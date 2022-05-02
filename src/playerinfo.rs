use bitstream_io::{BigEndian, BitWrite, BitWriter};
use osrs_buffer::ByteBuffer;
use slab::Slab;
use std::{cmp, error::Error, io::Write};

const MAX_PLAYERS: usize = 2047;
const UPDATE_GROUP_ACTIVE: i32 = 0;
const UPDATE_GROUP_INACTIVE: i32 = 1;

// An entry for a player, which contains data about all other players
struct PlayerInfoEntry {
    playerinfoother: Slab<PlayerInfoOther>,
}

// TODO: Consider just making this the PlayerInfoEntry, as this is kind of wasted
struct PlayerInfoOther {
    flags: i32,
    local: bool,
    coordinates: i32,
    reset: bool,
    remove_the_local_player: bool,
    masks: Vec<i32>,
    movement_steps: Vec<i32>,
}

impl PlayerInfoEntry {
    pub fn new() -> PlayerInfoEntry {
        PlayerInfoEntry {
            playerinfoother: Slab::new(),
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

    pub fn add_player(&mut self, coordinates: i32) -> Result<(), Box<dyn Error>> {
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
    ) -> Result<(), Box<dyn Error>> {
        let playerinfoentry = self
            .players
            .get_mut(playerinfo_id)
            .ok_or("failed getting playerinfoentry")?;

        playerinfoentry.playerinfoother.insert(PlayerInfoOther {
            flags: 0,
            local,
            coordinates,
            reset: false,
            remove_the_local_player: false,
            masks: Vec::new(),
            movement_steps: Vec::new(),
        });

        Ok(())
    }

    pub fn remove_player(&mut self, key: usize) -> Result<(), Box<dyn Error>> {
        self.players.remove(key);

        Ok(())
    }

    // Send player information to the player such as appearance etc
    pub fn process_player_info(&mut self, player_id: usize) {
        // TODO: Remove this, do proper checking instead in the local_player_info and world_player_info places, simply return if the player id does not exist
        if self.players.get(player_id).is_none() {
            return;
        }

        let mut main_buf = BitWriter::endian(Vec::new(), BigEndian);
        // Supply the mask buffer instead, as to prevent this big ass allocation
        let mut mask_buf = ByteBuffer::new(60000);

        let mut local = 0;
        let mut added = 0;

        local += self
            .local_player_info(player_id, &mut main_buf, &mut mask_buf, UPDATE_GROUP_ACTIVE)
            .unwrap();
        main_buf.byte_align().unwrap();

        local += self
            .local_player_info(
                player_id,
                &mut main_buf,
                &mut mask_buf,
                UPDATE_GROUP_INACTIVE,
            )
            .unwrap();
        main_buf.byte_align().unwrap();

        /*added += world_player_info(
            world,
            player_id,
            &mut main_buf,
            &mut mask_buf,
            UPDATE_GROUP_INACTIVE,
            local,
            added,
        );*/
        main_buf.byte_align().unwrap();

        /*world_player_info(
            world,
            player_id,
            &mut main_buf,
            &mut mask_buf,
            UPDATE_GROUP_ACTIVE,
            local,
            added,
        );*/
        main_buf.byte_align().unwrap();

        // Create buffer for sending GPI packet
        let mut send_buffer = ByteBuffer::new(60000);

        // Align the bitmode to make it byte oriented again
        main_buf.byte_align().unwrap();

        // Convert the main_buf into a writer
        let mut vec = main_buf.into_writer();

        // Write the mask_buf's data
        vec.write(&mask_buf.data[..mask_buf.write_pos]).unwrap();

        // Now write the bytes to the send_buffer
        send_buffer.write_bytes(&vec);

        // Group the records
        for i in 0..MAX_PLAYERS {
            self.group(player_id, i).ok();
        }
    }

    fn local_player_info(
        &mut self,
        player_id: usize,
        bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
        mask_buf: &mut ByteBuffer,
        update_group: i32,
    ) -> Result<i32, Box<dyn Error>> {
        let mut skip_count = 0;
        let mut local_players = 0;

        for other_player_id in 0..MAX_PLAYERS {
            // Grab the playerinfo
            let playerinfoentryother = self
                .players
                .get_mut(player_id)
                .unwrap()
                .playerinfoother
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
                remove_local_player(bit_buf, player_id, other_player_id);
                continue;
            }

            // Determine whether there is mask and movement updates
            let mask_update = playerinfoentryother.masks.len() > 0;
            let move_update = playerinfoentryother.movement_steps.len() > 0;

            // If there is a mask update, write them out
            if mask_update {
                write_mask_update(mask_buf, playerinfoentryother, other_player_id, 1);
            }

            // If there is either a mask or movement update, write a bit signifying so
            if mask_update || move_update {
                bit_buf.write_bit(true)?;
            }

            if move_update {
                write_local_movement(bit_buf, other_player_id, mask_update);
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
    ) -> Result<i32, Box<dyn Error>> {
        let mut count = 0;

        for i in offset..MAX_PLAYERS {
            // Grab the playerinfo
            let playerinfoentryother = self
                .players
                .get_mut(player_id)
                .unwrap()
                .playerinfoother
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
    ) -> Result<(), Box<dyn Error>> {
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
                println!("Skip count out of range error");
            }
            bit_buf.write(2, 3)?;
            bit_buf.write(11, cmp::min(MAX_PLAYERS, skip_count as usize) as u32)?;
        }

        Ok(())
    }

    fn group(&mut self, player_id: usize, index: usize) -> Result<(), Box<dyn Error>> {
        let playerinfoentryother = self
            .players
            .get_mut(player_id)
            .ok_or("failed getting playerinfoentry")?
            .playerinfoother
            .get_mut(index)
            .ok_or("failed playerinfoother")?;

        playerinfoentryother.flags >>= 1;

        if playerinfoentryother.reset {
            playerinfoentryother.flags = 0;
            playerinfoentryother.coordinates = 0;
            playerinfoentryother.local = false;
            playerinfoentryother.reset = false;
        }

        Ok(())
    }
}

fn write_mask_update(
    mask_buf: &mut ByteBuffer,
    playerinfo: &PlayerInfoOther,
    target_id: usize,
    mask_packets: i32,
) {
}

fn remove_local_player(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
    player_id: usize,
    target_id: usize,
) {
    let new_coordinates = 123;
    let record_coordinates = 12311;

    let coordinate_change = new_coordinates != record_coordinates;

    bit_buf.write_bit(true).unwrap();
    bit_buf.write_bit(false).unwrap();
    bit_buf.write(2, 0).unwrap();
    bit_buf.write_bit(coordinate_change).unwrap();

    if coordinate_change {
        write_coordinate_multiplier(bit_buf, record_coordinates, new_coordinates).unwrap();
    }
}

fn write_coordinate_multiplier(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
    old_multiplier: i32,
    new_multiplier: i32,
) -> Result<(), Box<dyn Error>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_player_test() {
        let mut playerinfo = PlayerInfo::new();
        playerinfo.add_player(123).unwrap();

        assert_eq!(playerinfo.players.len(), 1);
    }
}

fn write_local_movement(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
    target_id: usize,
    mask_update: bool,
) -> Option<()> {
    let direction_diff_x = [-1, 0, 1, -1, 1, -1, 0, 1];
    let direction_diff_y = [-1, -1, -1, 0, 0, 1, 1, 1];

    let curr_coords = 123;
    let last_coords = 124;

    let diff_x = 111;
    let diff_y = 222;
    let diff_level = 333;

    let large_change = false;
    let teleport = large_change || false;

    bit_buf.write_bit(mask_update).ok()?;
    if teleport {
        // SKIP TELEPORT FOR NOW
        bit_buf.write(2, 3).ok()?;
        bit_buf.write_bit(large_change).ok()?;
        bit_buf.write(2, diff_level & 0x3).ok()?;

        if large_change {
            bit_buf.write(14, diff_x & 0x3FFF).ok()?;
            bit_buf.write(14, diff_y & 0x3FFF).ok()?;
        } else {
            bit_buf.write(5, diff_x & 0x1F).ok()?;
            bit_buf.write(5, diff_y & 0x1F).ok()?;
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

    Some(())
}

fn write_mask_update_signal(
    bit_buf: &mut BitWriter<Vec<u8>, bitstream_io::BigEndian>,
) -> Result<(), Box<dyn Error>> {
    bit_buf.write(1, 1)?;
    bit_buf.write(2, 0)?;

    Ok(())
}
