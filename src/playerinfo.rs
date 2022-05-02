use bitstream_io::{BigEndian, BitWrite, BitWriter};
use osrs_buffer::ByteBuffer;
use slab::Slab;
use std::{cmp, error::Error, io::Write};

const MAX_PLAYERS: usize = 2047;

const UPDATE_GROUP_ACTIVE: i32 = 0;
const UPDATE_GROUP_INACTIVE: i32 = 1;

fn testy1() {
    println!("Testy");
}

// An entry for a player, which contains data about all other players
struct PlayerInfoEntry {
    playerinfoother: Slab<PlayerInfoOther>,
}

struct PlayerInfoOther {
    flags: i32,
    local: bool,
    coordinates: i32,
    reset: bool,
    remove_the_local_player: bool,
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
        let playerinfo_id = self.players.insert(PlayerInfoEntry::new());

        self.setup_gpi(playerinfo_id, coordinates);

        Ok(())
    }

    fn setup_gpi(&mut self, playerinfo_id: usize, coordinates: i32) {
        for playerinfo in 0..MAX_PLAYERS {
            if playerinfo_id == playerinfo {
                self.add_update_record(playerinfo_id, true, coordinates)
                    .expect("failed adding update record for local player");
            }
            self.add_update_record(playerinfo_id, false, 0)
                .expect("failed adding update record for external player");
        }
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
        let mut send_buffer: ByteBuffer = ByteBuffer::new(60000);

        // Align the bitmode to make it byte oriented again
        main_buf.byte_align().unwrap();

        // Convert the main_buf into a writer
        let mut vec = main_buf.into_writer();

        /*println!("Bit buffer:");
        for b in vec.iter() {
            print!("{:#01X} ", b);
        }
        println!("");
        println!("Mask buffer, write pos = {}:", mask_buf.write_pos);
        for b in 0..mask_buf.write_pos {
            print!("{:#01X} ", mask_buf.data.get(b).unwrap());
        }
        println!("");*/

        // Write the mask_buf's data
        vec.write(&mask_buf.data[..mask_buf.write_pos]).unwrap();

        //println!("Vec length: {}", vec.len());
        //println!("Data length: {}", send_buffer.data.len());

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
                continue;
            }

            let mask_update = true;
            let move_update = true;

            if mask_update {
                // Write mask update
            }

            if mask_update || move_update {
                bit_buf.write_bit(true)?;
            }

            if move_update {
                // Write local movement
            } else if mask_update {
                // Write mask update signal
            } else {
                playerinfoentryother.flags |= 0x2;
                skip_count = self.local_skip_count(update_group, player_id, other_player_id + 1)?;
                self.write_skip_count(bit_buf, skip_count);
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
    ) /*-> Result<(), Box<dyn Error>>*/
    {
        bit_buf.write(1, 0).unwrap();

        if skip_count == 0 {
            bit_buf.write(2, skip_count as u32).unwrap();
        } else if skip_count < 32 {
            bit_buf.write(2, 1).unwrap();
            bit_buf.write(5, skip_count as u32).unwrap();
        } else if skip_count < 256 {
            bit_buf.write(2, 2).unwrap();
            bit_buf.write(8, skip_count as u32).unwrap();
        } else {
            if skip_count > MAX_PLAYERS as i32 {
                println!("Skip count out of range error");
            }
            bit_buf.write(2, 3).unwrap();
            bit_buf
                .write(11, cmp::min(MAX_PLAYERS, skip_count as usize) as u32)
                .unwrap();
        }
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
