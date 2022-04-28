use bitstream_io::{BigEndian, BitWrite, BitWriter};
use osrs_buffer::ByteBuffer;
use slab::Slab;
use std::{error::Error, io::Write};

const MAX_PLAYERS: usize = 2047;

const UPDATE_GROUP_ACTIVE: i32 = 0;
const UPDATE_GROUP_INACTIVE: i32 = 1;

// An entry for a player, which contains data about all other players
struct PlayerInfoEntry {
    flags: Slab<i32>,
    local: Slab<bool>,
    coordinates: Slab<i32>,
    reset: Slab<bool>,
}

impl PlayerInfoEntry {
    pub fn new() -> PlayerInfoEntry {
        PlayerInfoEntry {
            flags: Slab::new(),
            local: Slab::new(),
            coordinates: Slab::new(),
            reset: Slab::new(),
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

        playerinfoentry.flags.insert(0);
        playerinfoentry.local.insert(local);
        playerinfoentry.coordinates.insert(coordinates);
        playerinfoentry.reset.insert(false);

        Ok(())
    }

    pub fn remove_player(&mut self, key: usize) -> Result<(), Box<dyn Error>> {
        self.players.remove(key);

        Ok(())
    }

    // Send player information to the player such as appearance etc
    pub fn process_player_info(&self, player_id: usize) {
        // TODO: Remove this, do proper checking instead in the local_player_info and world_player_info places, simply return if the player id does not exist
        if self.players.get(player_id).is_none() {
            return;
        }

        let mut main_buf = BitWriter::endian(Vec::new(), BigEndian);
        // Supply the mask buffer instead, as to prevent this big ass allocation
        let mut mask_buf = ByteBuffer::new(60000);

        let mut local = 0;
        let mut added = 0;

        /*local += local_player_info(
            world,
            player_id,
            &mut main_buf,
            &mut mask_buf,
            UPDATE_GROUP_ACTIVE,
        );*/
        main_buf.byte_align().unwrap();

        /*local += local_player_info(
            world,
            player_id,
            &mut main_buf,
            &mut mask_buf,
            UPDATE_GROUP_INACTIVE,
        );*/
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
            self.group(player_id, i);
        }
    }

    fn group(&self, player_id: usize, index: usize) {
        /*
        *world
            .players
            .get_mut(player_id)
            .unwrap()
            .update_record_flags
            .get_mut(index)
            .unwrap() >>= 1;

        if has_record_been_reset(world, player_id, index) {
            *world
                .players
                .get_mut(player_id)
                .unwrap()
                .update_record_flags
                .get_mut(index)
                .unwrap() = 0;

            *world
                .players
                .get_mut(player_id)
                .unwrap()
                .update_record_coordinates
                .get_mut(index)
                .unwrap() = 0;

            *world
                .players
                .get_mut(player_id)
                .unwrap()
                .update_record_local
                .get_mut(index)
                .unwrap() = false;

            *world
                .players
                .get_mut(player_id)
                .unwrap()
                .update_record_reset
                .get_mut(index)
                .unwrap() = false;
        }
        */
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
