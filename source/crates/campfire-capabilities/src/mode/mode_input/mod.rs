use campfire_sim::{Capability, Command};

use crate::mode::mode_data::InputType;

/// A player's mode input, the body of a `mode` command: its name, then its value in the format of
/// the type the mode declares for that name, in postcard: a string, or a list of strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeInput<'a> {
    pub name: &'a str,
    pub value: InputValue<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputValue<'a> {
    String(&'a str),
    StringList(Vec<&'a str>),
}

impl<'a> ModeInput<'a> {
    /// The owner of mode inputs.
    pub const CAPABILITY: Capability = Capability::Mode;

    pub fn encode(&self) -> Vec<u8> {
        let mut body = postcard::to_allocvec(self.name).expect("a string always encodes");
        let value = match &self.value {
            InputValue::String(text) => postcard::to_allocvec(text),
            InputValue::StringList(texts) => postcard::to_allocvec(texts),
        };
        body.extend(value.expect("strings always encode"));
        body
    }

    /// The input in `body`, when its name is one of `types` and its value is exactly of the
    /// type declared for it: a client can send any bytes.
    pub fn decode(
        body: &'a [u8],
        types: impl Fn(&str) -> Option<InputType>,
    ) -> Option<ModeInput<'a>> {
        let (name, rest): (&str, _) = postcard::take_from_bytes(body).ok()?;
        let (value, rest) = match types(name)? {
            InputType::String => {
                let (text, rest) = postcard::take_from_bytes(rest).ok()?;
                (InputValue::String(text), rest)
            }
            InputType::StringList => {
                let (texts, rest) = postcard::take_from_bytes(rest).ok()?;
                (InputValue::StringList(texts), rest)
            }
        };
        rest.is_empty().then_some(ModeInput { name, value })
    }

    /// A payload of `inputs` alone, one command each.
    pub fn payload(inputs: &[ModeInput<'_>]) -> Vec<u8> {
        let bodies: Vec<Vec<u8>> = inputs.iter().map(ModeInput::encode).collect();
        Command::payload(ModeInput::CAPABILITY, &bodies)
    }
}

#[cfg(test)]
mod tests;
