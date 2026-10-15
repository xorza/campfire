use campfire_common::{Meaning, Value32};

/// The BLAKE3 hash that links a player's input to the one before it.
pub type InputHash = Value32<InputHashMeaning>;

/// The meaning of a `InputHash`.
#[derive(Debug)]
pub enum InputHashMeaning {}

impl Meaning for InputHashMeaning {
    const NAME: &'static str = "InputHash";
}
