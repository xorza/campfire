use campfire_common::{Binary, Taken};
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
        let mut body = Binary::encode(self.name);
        match &self.value {
            InputValue::String(text) => Binary::encode_into(text, &mut body),
            InputValue::StringList(texts) => Binary::encode_into(texts, &mut body),
        }
        body
    }

    /// The input in `body`, when its name is one of `types` and its value is exactly of the
    /// type declared for it: a client can send any bytes.
    pub fn decode(
        body: &'a [u8],
        types: impl Fn(&str) -> Option<InputType>,
    ) -> Option<ModeInput<'a>> {
        let Taken { value: name, rest } = Binary::take::<&str>(body).ok()?;
        let Taken { value, rest } = match types(name)? {
            InputType::String => {
                let text = Binary::take::<&str>(rest).ok()?;
                Taken {
                    value: InputValue::String(text.value),
                    rest: text.rest,
                }
            }
            InputType::StringList => {
                let texts = Binary::take::<Vec<&str>>(rest).ok()?;
                Taken {
                    value: InputValue::StringList(texts.value),
                    rest: texts.rest,
                }
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
