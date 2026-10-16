use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// How a build's site grows, as its `construct` names it: by itself, as long as one builder
/// builds it, or with every builder that builds it, at the rate `builders` gives for their count
/// from one, the last entry for any count past it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructData {
    Alone,
    Builder,
    Builders(Vec<Num>),
}

/// `"alone"`, `"builder"`, or `{ builders = [...] }`, a positive rate for each count, one at
/// least.
impl<'de> Deserialize<'de> for ConstructData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Style {
            Alone,
            Builder,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Table {
            builders: Vec<Num>,
        }
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Style(Style),
            Table(Table),
        }
        match Written::deserialize(deserializer)? {
            Written::Style(Style::Alone) => Ok(Self::Alone),
            Written::Style(Style::Builder) => Ok(Self::Builder),
            Written::Table(Table { builders }) => {
                if !builders.is_empty() && builders.iter().all(|&rate| rate > Num::ZERO) {
                    Ok(Self::Builders(builders))
                } else {
                    Err(D::Error::custom(
                        "a builders table holds a positive rate for each count, one at least",
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Toml, TomlError};

    use super::*;

    #[test]
    fn a_style_reads_as_its_name_or_its_table_of_positive_rates() {
        let read = |text: &str| -> Result<ConstructData, TomlError> {
            #[derive(Debug, Deserialize)]
            struct Field {
                construct: ConstructData,
            }
            Toml::parse::<Field>(text).map(|field| field.construct)
        };
        assert_eq!(
            read(r#"construct = "alone""#).unwrap(),
            ConstructData::Alone
        );
        assert_eq!(
            read(r#"construct = "builder""#).unwrap(),
            ConstructData::Builder
        );
        let rates = read(r#"construct = { builders = ["1", "1.5"] }"#).unwrap();
        assert_eq!(
            rates,
            ConstructData::Builders(vec![Num::ONE, Num::ONE + Num::HALF])
        );
        for flawed in [
            r#"construct = "crew""#,
            "construct = { builders = [] }",
            r#"construct = { builders = ["0"] }"#,
            r#"construct = { builders = ["-1"] }"#,
            r#"construct = { builders = ["1"], rest = 2 }"#,
        ] {
            assert!(read(flawed).is_err(), "{flawed}");
        }
    }
}
