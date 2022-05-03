use anyhow::{anyhow, Context, Result};
use bitstream_io::{BigEndian, BitWrite, BitWriter};
use osrs_buffer::ByteBuffer;
use slab::Slab;
use std::{cmp, io::Write};

const MAX_PLAYERS: usize = 2047;
const UPDATE_GROUP_ACTIVE: i32 = 0;
const UPDATE_GROUP_INACTIVE: i32 = 1;

pub enum PlayerMask {
    AppearanceMask(AppearanceMask),
    DirectionMask(DirectionMask),
}

pub struct AppearanceMask {
    pub gender: i8,
    pub skull: bool,
    pub overhead_prayer: i8,
    //pub npc: i32,
    //pub looks: PlayerLooks,
    pub head: i16,
    pub cape: i16,
    pub neck: i16,
    pub weapon: i16,
    pub body: i16,
    pub shield: i16,
    pub arms: i16,
    pub is_full_body: bool,
    pub legs: i16,
    pub hair: i16,
    pub covers_hair: bool,
    pub hands: i16,
    pub feet: i16,
    pub covers_face: bool,
    pub beard: i16,
    pub colors_hair: i8,
    pub colors_torso: i8,
    pub colors_legs: i8,
    pub colors_feet: i8,
    pub colors_skin: i8,
    pub weapon_stance_stand: i16,
    pub weapon_stance_turn: i16,
    pub weapon_stance_walk: i16,
    pub weapon_stance_turn180: i16,
    pub weapon_stance_turn90cw: i16,
    pub weapon_stance_turn90ccw: i16,
    pub weapon_stance_run: i16,
    pub username: String,
    pub combat_level: i8,
    pub skill_id_level: i16,
    pub hidden: i8,
}

pub struct DirectionMask {
    pub direction: i16,
}

// An entry for a player, which contains data about all other players
struct PlayerInfoEntry {
    playerinfodata: Slab<PlayerInfoData>,
}

// TODO: Consider just making this the PlayerInfoEntry, as this is kind of wasted
struct PlayerInfoData {
    flags: i32,
    local: bool,
    coordinates: i32,
    reset: bool,
    remove_the_local_player: bool,
    masks: Vec<PlayerMask>,
    movement_steps: Vec<i32>,
    displaced: bool,
}

impl PlayerInfoEntry {
    pub fn new() -> PlayerInfoEntry {
        PlayerInfoEntry {
            playerinfodata: Slab::new(),
        }
    }
}

pub struct PlayerInfo {
    players: Slab<PlayerInfoEntry>,
}

impl PlayerInfo {
    pub fn new() -> PlayerInfo {
        PlayerInfo {
            players: Slab::new(),
        }
    }

    pub fn add_player(&mut self, coordinates: i32) -> Result<()> {
        println!("Got a vacant key yo {}", self.players.vacant_key());

        // Insert the new player into the slab, retrieve their id
        let playerinfo_id = self.players.insert(PlayerInfoEntry::new());

        // Generate the playerinfo for the given player
        for playerinfo in 0..MAX_PLAYERS {
            if playerinfo_id == playerinfo {
                self.add_update_record(playerinfo_id, true, coordinates)
                    .expect("failed adding update record for local player");
            }
            self.add_update_record(playerinfo_id, false, 0)
                .expect("failed adding update record for external player");
        }

        Ok(())
    }

    fn add_update_record(
        &mut self,
        playerinfo_id: usize,
        local: bool,
        coordinates: i32,
    ) -> Result<()> {
        let playerinfoentry = self
            .players
            .get_mut(playerinfo_id)
            .context("failed getting playerinfoentry")?;

        playerinfoentry.playerinfodata.insert(PlayerInfoData {
            flags: 0,
            local,
            coordinates,
            reset: false,
            remove_the_local_player: false,
            masks: Vec::new(),
            movement_steps: Vec::new(),
            displaced: false,
        });

        Ok(())
    }

    pub fn remove_player(&mut self, key: usize) -> Result<()> {
        self.players.remove(key);

        Ok(())
    }

    // Send player information to the player such as appearance etc
    pub fn process_player_info(&mut self, player_id: usize) -> Result<()> {
        // TODO: Remove this, do proper checking instead in the local_player_info and world_player_info places, simply return if the player id does not exist
        if self.players.get(player_id).is_none() {
            return Ok(());
        }

        let mut main_buf = BitWriter::endian(Vec::new(), BigEndian);
        // Supply the mask buffer instead, as to prevent this big ass allocation
        let mut mask_buf = ByteBuffer::new(60000);

        let mut local = 0;
        let mut added = 0;

        local +=
            self.local_player_info(player_id, &mut main_buf, &mut mask_buf, UPDATE_GROUP_ACTIVE)?;
        main_buf.byte_align()?;

        local += self.local_player_info(
            player_id,
            &mut main_buf,
            &mut mask_buf,
            UPDATE_GROUP_INACTIVE,
        )?;
        main_buf.byte_align()?;

        /*added += world_player_info(
            world,
            player_id,
            &mut main_buf,
            &mut mask_buf,
            UPDATE_GROUP_INACTIVE,
            local,
            added,
        );*/
        main_buf.byte_align()?;

        /*world_player_info(
            world,
            player_id,
            &mut main_buf,
            &mut mask_buf,
            UPDATE_GROUP_ACTIVE,
            local,
            added,
        );*/
        main_buf.byte_align()?;

        // Create buffer for sending GPI packet
        let mut send_buffer = ByteBuffer::new(60000);

        // Align the bitmode to make it byte oriented again
        main_buf.byte_align()?;

        // Convert the main_buf into a writer
        let mut vec = main_buf.into_writer();

        // Write the mask_buf's data
        vec.write_all(&mask_buf.data[..mask_buf.write_pos])?;

        // Now write the bytes to the send_buffer
        send_buffer.write_bytes(&vec);

        // Group the records
        for i in 0..MAX_PLAYERS {
            self.group(player_id, i).ok();
        }

        Ok(())
    }

    fn local_player_info(
        &mut self,
        player_id: usize,
        bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
        mask_buf: &mut ByteBuffer,
        update_group: i32,
    ) -> Result<i32> {
        let mut skip_count = 0;
        let mut local_players = 0;

        for other_player_id in 0..MAX_PLAYERS {
            // Grab the playerinfo
            let playerinfoentryother = self
                .players
                .get_mut(player_id)
                .unwrap()
                .playerinfodata
                .get_mut(other_player_id)
                .unwrap();

            // Test whether the playerinfo is local, and whether it is in the correct update group (active, inactive)
            if !(playerinfoentryother.local && (update_group & 0x1) == playerinfoentryother.flags) {
                continue;
            }

            // Check whether entries should be skipped
            if skip_count > 0 {
                skip_count -= 1;
                playerinfoentryother.flags |= 0x2;
                continue;
            }

            // Increment the local players by 1
            local_players += 1;

            // Check whether the local player should be removed and turned into a global player
            if playerinfoentryother.remove_the_local_player {
                playerinfoentryother.reset = true;
                remove_local_player(bit_buf, &playerinfoentryother)?;
                continue;
            }

            // Determine whether there is mask and movement updates
            let mask_update = !playerinfoentryother.masks.is_empty();
            let move_update =
                !playerinfoentryother.movement_steps.is_empty() || playerinfoentryother.displaced;

            // If there is a mask update, write them out
            if mask_update {
                write_mask_update(mask_buf, playerinfoentryother);
            }

            // If there is either a mask or movement update, write a bit signifying so
            if mask_update || move_update {
                bit_buf.write_bit(true)?;
            }

            if move_update {
                write_local_movement(bit_buf, other_player_id, mask_update)
                    .expect("failed writing local movement");
            } else if mask_update {
                write_mask_update_signal(bit_buf).expect("failed writing mask update signal");
            } else {
                playerinfoentryother.flags |= 0x2;
                skip_count = self.local_skip_count(update_group, player_id, other_player_id + 1)?;
                self.write_skip_count(bit_buf, skip_count).ok();
            }
        }

        Ok(local_players)
    }

    fn local_skip_count(
        &mut self,
        update_group: i32,
        player_id: usize,
        offset: usize,
    ) -> Result<i32> {
        let mut count = 0;

        for i in offset..MAX_PLAYERS {
            // Grab the playerinfo
            let playerinfoentryother = self
                .players
                .get_mut(player_id)
                .unwrap()
                .playerinfodata
                .get_mut(i)
                .unwrap();

            // Return if the playerinfo is not in this group
            if !(playerinfoentryother.local && (update_group & 0x1) == playerinfoentryother.flags) {
                continue;
            }

            // Break if a player needs to be updated
            let is_update_required = true;
            if is_update_required {
                break;
            }

            // Increment the skip count by 1
            count += 1;
        }

        Ok(count)
    }

    fn write_skip_count(
        &self,
        bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
        skip_count: i32,
    ) -> Result<()> {
        bit_buf.write(1, 0)?;

        if skip_count == 0 {
            bit_buf.write(2, skip_count as u32)?;
        } else if skip_count < 32 {
            bit_buf.write(2, 1)?;
            bit_buf.write(5, skip_count as u32)?;
        } else if skip_count < 256 {
            bit_buf.write(2, 2)?;
            bit_buf.write(8, skip_count as u32)?;
        } else {
            if skip_count > MAX_PLAYERS as i32 {
                return Err(anyhow!("Skip count out of range error"));
            }
            bit_buf.write(2, 3)?;
            bit_buf.write(11, cmp::min(MAX_PLAYERS, skip_count as usize) as u32)?;
        }

        Ok(())
    }

    fn group(&mut self, player_id: usize, index: usize) -> Result<()> {
        // Get the playerinfo
        let playerinfoentryother = self
            .players
            .get_mut(player_id)
            .context("failed getting playerinfoentry")?
            .playerinfodata
            .get_mut(index)
            .context("failed playerinfoother")?;

        // Shift its flags
        playerinfoentryother.flags >>= 1;

        // If reset is set, reset the playerinfo
        if playerinfoentryother.reset {
            playerinfoentryother.flags = 0;
            playerinfoentryother.coordinates = 0;
            playerinfoentryother.local = false;
            playerinfoentryother.reset = false;
        }

        Ok(())
    }
}

fn write_mask_update(mask_buf: &mut ByteBuffer, playerinfo: &PlayerInfoData) {
    let mut mask: i32 = 0;

    // TODO: When assigning masks to players, OR the value of the mask on them instead of this double loop
    for mask_order in 0..=11 {
        for record in playerinfo.masks.iter() {
            match mask_order {
                0 => match record {
                    // Movement forced
                    _ => (),
                },
                1 => match record {
                    // Spot animation
                    _ => (),
                },
                2 => match record {
                    // Sequence
                    _ => (),
                },
                3 => match record {
                    PlayerMask::AppearanceMask(p) => {
                        mask |= 0x2;
                    }
                    _ => (),
                },
                4 => match record {
                    // Shout
                    _ => (),
                },
                5 => match record {
                    // Lock turn to
                    _ => (),
                },
                6 => match record {
                    // Movement cached
                    _ => (),
                },
                7 => match record {
                    // Chat
                    _ => (),
                },
                8 => match record {
                    // Name modifiers
                    _ => (),
                },
                9 => match record {
                    // Hit
                    _ => (),
                },
                10 => match record {
                    // Movement temporary
                    _ => (),
                },
                11 => match record {
                    PlayerMask::DirectionMask(p) => {
                        mask |= 0x8;
                    }
                    _ => (),
                },
                _ => (),
            }
        }
    }

    // TODO: Calculate mask as playermasks are set instead of doing this double iteration
    if mask >= 0xFF {
        mask_buf.write_i8((mask | 0x40) as i8);
        mask_buf.write_i8((mask >> 8) as i8);
    } else {
        mask_buf.write_i8(mask as i8);
    }

    // Now write masks
    for mask_order in 0..=11 {
        // TODO: Consider if this should be mutable. Very likely it won't need to be
        for record in playerinfo.masks.iter() {
            match mask_order {
                0 => match record {
                    // Movement forced
                    _ => (),
                },
                1 => match record {
                    // Spot animation
                    _ => (),
                },
                2 => match record {
                    // Sequence
                    _ => (),
                },
                3 => match record {
                    PlayerMask::AppearanceMask(p) => write_appearance_mask(&p, mask_buf),
                    _ => (),
                },
                4 => match record {
                    // Shout
                    _ => (),
                },
                5 => match record {
                    // Lock turn to
                    _ => (),
                },
                6 => match record {
                    // Movement cached
                    _ => (),
                },
                7 => match record {
                    // Chat
                    _ => (),
                },
                8 => match record {
                    // Name modifiers
                    _ => (),
                },
                9 => match record {
                    // Hit
                    _ => (),
                },
                10 => match record {
                    // Movement temporary
                    _ => (),
                },
                11 => match record {
                    PlayerMask::DirectionMask(p) => write_direction_mask(&p, mask_buf),
                    _ => (),
                },
                _ => (),
            }
        }
    }
}

fn remove_local_player(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
    playerinfo: &PlayerInfoData,
) -> Result<()> {
    let new_coordinates = 123;
    let record_coordinates = 12311;

    let coordinate_change = new_coordinates != record_coordinates;

    bit_buf.write_bit(true)?;
    bit_buf.write_bit(false)?;
    bit_buf.write(2, 0)?;
    bit_buf.write_bit(coordinate_change)?;

    if coordinate_change {
        write_coordinate_multiplier(bit_buf, record_coordinates, new_coordinates)?;
    }

    Ok(())
}

fn write_coordinate_multiplier(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
    old_multiplier: i32,
    new_multiplier: i32,
) -> Result<()> {
    let current_multiplier_y = new_multiplier & 0xFF;
    let current_multiplier_x = (new_multiplier >> 8) & 0xFF;
    let current_level = (new_multiplier >> 8) & 0x3;

    let last_multiplier_y = old_multiplier & 0xFF;
    let last_multiplier_x = (old_multiplier >> 8) & 0xFF;
    let last_level = (old_multiplier >> 8) & 0x3;

    let diff_x = current_multiplier_x - last_multiplier_x;
    let diff_y = current_multiplier_y - last_multiplier_y;
    let diff_level = current_level - last_level;

    let level_change = diff_level != 0;
    let small_change = diff_x.abs() <= 1 && diff_y.abs() <= 1;

    if level_change {
        bit_buf.write(2, 1)?;
        bit_buf.write(2, diff_level as u32)?;
    } else if small_change {
        let direction;

        if diff_x == -1 && diff_y == -1 {
            direction = 0;
        } else if diff_x == 1 && diff_y == -1 {
            direction = 2;
        } else if diff_x == -1 && diff_y == 1 {
            direction = 5;
        } else if diff_x == 1 && diff_y == 1 {
            direction = 7;
        } else if diff_y == -1 {
            direction = 1;
        } else if diff_x == -1 {
            direction = 3;
        } else if diff_x == 1 {
            direction = 4;
        } else {
            direction = 6;
        }

        bit_buf.write(2, 2)?;
        bit_buf.write(2, diff_level as u32)?;
        bit_buf.write(3, direction)?;
    } else {
        bit_buf.write(2, 3)?;
        bit_buf.write(2, diff_level as u32)?;
        bit_buf.write(8, diff_x as u32 & 0xFF)?;
        bit_buf.write(8, diff_y as u32 & 0xFF)?;
    }

    Ok(())
}

fn write_local_movement(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
    target_id: usize,
    mask_update: bool,
) -> Result<()> {
    let direction_diff_x = [-1, 0, 1, -1, 1, -1, 0, 1];
    let direction_diff_y = [-1, -1, -1, 0, 0, 1, 1, 1];

    let curr_coords = 123;
    let last_coords = 124;

    let diff_x = 111;
    let diff_y = 222;
    let diff_level = 333;

    let large_change = false;
    let teleport = large_change || false;

    bit_buf.write_bit(mask_update)?;
    if teleport {
        // SKIP TELEPORT FOR NOW
        bit_buf.write(2, 3)?;
        bit_buf.write_bit(large_change)?;
        bit_buf.write(2, diff_level & 0x3)?;

        if large_change {
            bit_buf.write(14, diff_x & 0x3FFF)?;
            bit_buf.write(14, diff_y & 0x3FFF)?;
        } else {
            bit_buf.write(5, diff_x & 0x1F)?;
            bit_buf.write(5, diff_y & 0x1F)?;
        }
    } else {
        /*let steps = &mut world.players.get_mut(target_id)?.movement_queue.next_steps;
        let walk_step = steps.get(0)?;
        let walk_rotation = get_direction_rotation(&walk_step.dir);

        let mut dx = *direction_diff_x.get(walk_rotation as usize)?;
        let mut dy = *direction_diff_y.get(walk_rotation as usize)?;

        let mut running = false;
        let mut direction = 0;

        if let Some(run_step) = steps.get(1) {
            println!("WHY ARE YOU RUNNING 2");
            let run_rotation = get_direction_rotation(&run_step.dir);

            dx += *direction_diff_x.get(run_rotation as usize)?;
            dy += *direction_diff_y.get(run_rotation as usize)?;

            if let Some(run_dir) = run_dir(dx, dy) {
                direction = run_dir;
                running = true;
            }
        }

        if !running {
            if let Some(walk_dir) = walk_dir(dx, dy) {
                direction = walk_dir;
            }
        }

        if running {
            bit_buf.write(2, 2).ok()?;
            bit_buf.write(4, direction).ok()?;
        } else {
            bit_buf.write(2, 1).ok()?;
            bit_buf.write(3, direction).ok()?;
        }

        steps.clear();*/
    }

    Ok(())
}

fn write_mask_update_signal(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
) -> Result<()> {
    bit_buf.write(1, 1)?;
    bit_buf.write(2, 0)?;

    Ok(())
}

fn write_direction_mask(direction_mask: &DirectionMask, mask_buf: &mut ByteBuffer) {
    mask_buf.write_i16_add(direction_mask.direction);
}

fn write_appearance_mask(appearance_mask: &AppearanceMask, mask_buf: &mut ByteBuffer) {
    let mut temp_buf: ByteBuffer = ByteBuffer::new(200);

    temp_buf.write_i8(appearance_mask.gender);
    if appearance_mask.skull {
        temp_buf.write_i8(1)
    } else {
        temp_buf.write_i8(-1)
    }

    temp_buf.write_i8(appearance_mask.overhead_prayer);

    // Equipment here, skipped for now
    temp_buf.write_i8(0); // Head
    temp_buf.write_i8(0); // Cape
    temp_buf.write_i8(0); // Neck
    temp_buf.write_i8(0); // Weapon

    temp_buf.write_i16(256 + 18); // Torso
    temp_buf.write_i8(0); // Shield
    temp_buf.write_i16(256 + appearance_mask.arms); // Arms
    temp_buf.write_i16(256 + appearance_mask.legs); // Legs
    temp_buf.write_i16(256 + appearance_mask.hair); // Hair
    temp_buf.write_i16(256 + appearance_mask.hands); // Hands
    temp_buf.write_i16(256 + appearance_mask.feet); // Feet

    if appearance_mask.gender == 0 {
        temp_buf.write_i16(256 + appearance_mask.beard); // Beard
    } else {
        temp_buf.write_i16(0);
    }

    temp_buf.write_i8(appearance_mask.colors_hair);
    temp_buf.write_i8(appearance_mask.colors_torso);
    temp_buf.write_i8(appearance_mask.colors_legs);
    temp_buf.write_i8(appearance_mask.colors_feet);
    temp_buf.write_i8(appearance_mask.colors_skin);

    temp_buf.write_i16(appearance_mask.weapon_stance_stand);
    temp_buf.write_i16(appearance_mask.weapon_stance_turn);
    temp_buf.write_i16(appearance_mask.weapon_stance_walk);
    temp_buf.write_i16(appearance_mask.weapon_stance_turn180);
    temp_buf.write_i16(appearance_mask.weapon_stance_turn90cw);
    temp_buf.write_i16(appearance_mask.weapon_stance_turn90ccw);
    temp_buf.write_i16(appearance_mask.weapon_stance_run);

    temp_buf.write_string_null_terminated(&appearance_mask.username);
    temp_buf.write_i8(appearance_mask.combat_level);
    temp_buf.write_i16(appearance_mask.skill_id_level);
    temp_buf.write_i8(appearance_mask.hidden);

    mask_buf.write_i8(temp_buf.write_pos as i8);

    mask_buf.write_bytes_reversed_add(&temp_buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_player_test() -> Result<()> {
        let mut playerinfo = PlayerInfo::new();
        playerinfo.add_player(123)?;

        assert_eq!(playerinfo.players.len(), 1);

        Ok(())
    }

    #[test]
    fn playerinfo_test() -> Result<()> {
        let mut playerinfo = PlayerInfo::new();
        playerinfo.add_player(123)?;

        playerinfo.process_player_info(0).unwrap();

        Ok(())
    }
}
