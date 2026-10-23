//! A session's client: where it is in its session, its chain's history and the inputs it sent,
//! its signers, the server it pins, and its data directory with the receipts it keeps.

pub(crate) mod chain_history;
pub(crate) mod client_dir;
pub(crate) mod client_layout;
pub(crate) mod join_state;
pub(crate) mod player_signer;
pub(crate) mod receipt_writer;
pub(crate) mod sent_inputs;
pub(crate) mod server_pin;
pub(crate) mod session_signer;
pub(crate) mod sim_client_world;
