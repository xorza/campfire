use std::str::FromStr;

use campfire_values::{DeclaredName, Scalar};

use crate::error::OptionValueError;

/// A host's pick of an option by its value, as `--option` writes it: `name=value`, the value an
/// integer or a decimal (10-sessions.md, A pick is given by its value). The mode's options
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
mod tests {
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_pick_reads_as_a_name_and_a_number() {
        let read = |text: &str| text.parse::<OptionValue>();
        let pick = |name: &str, value| OptionValue {
            name: DeclaredName::new(name).unwrap(),
            value,
        };
        assert_eq!(
            read("start_money=50000"),
            Ok(pick("start_money", Scalar::Int(50000)))
        );
        assert_eq!(
            read("speed=0.5"),
            Ok(pick("speed", Scalar::Decimal(Num::HALF)))
        );
        assert_eq!(read("start_money"), Err(OptionValueError::NoValue));
        assert_eq!(
            read("Start=5"),
            Err(OptionValueError::Name("Start".to_owned()))
        );
        assert_eq!(
            read("start_money=lots"),
            Err(OptionValueError::Value("lots".to_owned()))
        );
    }
}
