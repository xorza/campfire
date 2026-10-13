use secp256k1::XOnlyPublicKey;

use crate::input_chain::InputChain;

/// Who controls a slot now, as the log follows it: a player, by their main key and their current
/// session key, with their chain as logged so far; a bot; no one; or no one but the player who
/// left it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    Player {
        main_key: [u8; 32],
        session_key: XOnlyPublicKey,
        chain: InputChain,
    },
    Bot,
    Open,
    Reserved,
}
