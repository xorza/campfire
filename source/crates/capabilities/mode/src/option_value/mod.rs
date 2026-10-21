use std::str::FromStr;

use campfire_common::DeclaredName;
use campfire_values::Scalar;

use crate::error::OptionValueError;

/// A host's pick of an option by its value, as `--option` writes it: `name=value`, the value an
/// integer or a decimal (08-sessions.md, A pick is given by its value). The mode's options
/// check it as a session opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionValue {
    pub name: DeclaredName,
    pub value: Scalar,
}

impl FromStr for OptionValue {
    type Err = OptionValueError;

    fn from_str(text: &str) -> Result<Self, OptionValueError> {
        let (name, value) = text.split_once('=').ok_or(OptionValueError::NoValue)?;
        let name =
            DeclaredName::new(name).ok_or_else(|| OptionValueError::Name(name.to_owned()))?;
        let value = value
            .parse()
            .map_err(|_| OptionValueError::Value(value.to_owned()))?;
        Ok(Self { name, value })
    }
}

#[cfg(test)]
mod tests;
