use std::array;
use std::num::NonZeroU32;

use secp256k1::{Keypair, Secp256k1, SecretKey};

use crate::seed_chain::SeedChain;

/// The tests' fixed identities: the key of one byte repeated, the keys of the tests' server, of
/// each player and of each spectator, the time they sign at, and the server's seed chain.
#[derive(Debug)]
pub struct TestKey;

/// A test player's keys: the main key that names them, and the session key it delegates to.
#[derive(Debug, Clone, Copy)]
pub struct PlayerKeys {
    pub main: Keypair,
    pub session: Keypair,
}

impl TestKey {
    /// The byte the key of the tests' servers repeats.
    const SERVER: u8 = 41;
    /// The players `player` gives keys to.
    pub const PLAYERS: u32 = 8;
    /// The Unix time, in seconds, every test signs at and its clocks start from.
    pub const NOW: u64 = 1_700_000_000;

    /// The key whose secret is 32 bytes of `byte`: every byte but 0 and 255 gives one, as 32
    /// bytes of 255 pass the curve's order.
    pub fn of(byte: u8) -> Keypair {
        let secret = SecretKey::from_byte_array(&[byte; 32]).expect("a secret key below the order");
        Keypair::from_secret_key(&Secp256k1::new(), &secret)
    }

    /// The key the tests' servers sign with.
    pub fn server() -> Keypair {
        TestKey::of(TestKey::SERVER)
    }

    /// The keys of every player by slot, each main key sorting after every lower slot's, as a
    /// server orders the joins of one frame by main key. The main keys take the odd bytes below
    /// `2 × PLAYERS`, and each session key the byte after its main key's.
    pub fn players() -> [PlayerKeys; TestKey::PLAYERS as usize] {
        let mut players: [PlayerKeys; TestKey::PLAYERS as usize] =
            array::from_fn(|slot| TestKey::pair(u8::try_from(2 * slot + 1).unwrap()));
        players.sort_by_key(|keys| keys.main.x_only_public_key().0.serialize());
        players
    }

    /// The keys of the player of `slot`, as `players` gives them.
    pub fn player(slot: u32) -> PlayerKeys {
        TestKey::players()[usize::try_from(slot).unwrap()]
    }

    /// The keys of the `n`th spectator, past every player's, the server's among them.
    pub fn spectator(n: u32) -> PlayerKeys {
        TestKey::pair(u8::try_from(100 + 2 * n).expect("a few spectators"))
    }

    /// The seed chain the tests' servers commit to, of room for `room` segments.
    pub const fn seed_chain(room: NonZeroU32) -> SeedChain {
        SeedChain::new([9; 32], room)
    }

    fn pair(main: u8) -> PlayerKeys {
        PlayerKeys {
            main: TestKey::of(main),
            session: TestKey::of(main + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_sort_by_slot_and_no_two_identities_share_a_key() {
        // The odd bytes 1 to 15 sort by their keys as 15, 1, 13, 3, 11, 9, 5, 7.
        let players = TestKey::players();
        assert_eq!(players[3].main, TestKey::player(3).main);
        let mains: Vec<[u8; 32]> = players
            .iter()
            .map(|keys| keys.main.x_only_public_key().0.serialize())
            .collect();
        assert!(mains.is_sorted());
        assert_eq!(players[0].main, TestKey::of(15));
        assert_eq!(players[0].session, TestKey::of(16));
        assert_eq!(players[7].main, TestKey::of(7));
        let spectators = [TestKey::spectator(0), TestKey::spectator(1)];
        assert_eq!(spectators[1].main, TestKey::of(102));
        assert_eq!(spectators[1].session, TestKey::of(103));
        let mut keys: Vec<Keypair> = players
            .iter()
            .chain(&spectators)
            .flat_map(|keys| [keys.main, keys.session])
            .chain([TestKey::server()])
            .collect();
        keys.sort_by_key(Keypair::secret_bytes);
        keys.dedup();
        assert_eq!(keys.len(), 2 * 8 + 2 * 2 + 1);
    }
}
