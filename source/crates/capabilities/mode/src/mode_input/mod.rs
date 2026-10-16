use campfire_codec::{Binary, Taken};
use campfire_sim::{Capability, Command};

use crate::mode_data::InputType;

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

    /// Appends the body of the input to `out`.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        Binary::encode_into(self.name, out);
        match &self.value {
            InputValue::String(text) => Binary::encode_into(text, out),
            InputValue::StringList(texts) => Binary::encode_into(texts, out),
        }
    }

    /// The input in `body`, when its name is one of `types` and its value is exactly of the
    /// type declared for it: a client can send any bytes.
    pub fn decode(body: &'a [u8], types: impl Fn(&str) -> Option<InputType>) -> Option<Self> {
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

    /// A payload of `inputs`, one command each.
    pub fn payload(inputs: &[ModeInput<'_>]) -> Vec<u8> {
        let bodies: Vec<Vec<u8>> = inputs
            .iter()
            .map(|input| {
                let mut body = Vec::new();
                input.encode_into(&mut body);
                body
            })
            .collect();
        Command::payload(ModeInput::CAPABILITY, &bodies)
    }

    /// Appends the payload of this input alone to `out`, its body written in `body`, which it
    /// clears first, as a sender keeps both so no input allocates.
    pub fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        body.clear();
        self.encode_into(body);
        let command = Command {
            capability: ModeInput::CAPABILITY,
            body,
        };
        Command::write(&[command], out);
    }
}

#[cfg(test)]
mod tests;
