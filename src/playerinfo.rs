use slab::Slab;
use std::error::Error;

const MAX_PLAYERS: usize = 2047;

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
                self.add_update_record(playerinfo_id, playerinfo, true, coordinates);
            }
            self.add_update_record(playerinfo_id, playerinfo, false, 0);
        }
    }

    fn add_update_record(
        &mut self,
        playerinfo_id: usize,
        index: usize,
        local: bool,
        coordinates: i32,
    ) {
        self.players.get_mut(playerinfo_id).unwrap().flags.insert(0);
    }

    pub fn remove_player(&mut self, key: usize) -> Result<(), Box<dyn Error>> {
        self.players.remove(key);

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
