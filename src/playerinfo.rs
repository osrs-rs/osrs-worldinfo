use slab::Slab;
use std::error::Error;

struct PlayerInfoEntry {}

pub struct PlayerInfo {
    players: Slab<PlayerInfoEntry>,
}

impl PlayerInfo {
    pub fn new() -> PlayerInfo {
        PlayerInfo {
            players: Slab::new(),
        }
    }

    pub fn add_player(&mut self) -> Result<(), Box<dyn Error>> {
        self.players.insert(PlayerInfoEntry {});

        Ok(())
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
        playerinfo.add_player().unwrap();

        assert_eq!(playerinfo.players.len(), 1);
    }
}
