use campfire_math::Num;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::values::scalar::Scalar;

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
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ConstructData, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Style {
            Alone,
            Builder,
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Table {
            builders: Vec<Scalar>,
        }
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Style(Style),
            Table(Table),
        }
        match Written::deserialize(deserializer)? {
            Written::Style(Style::Alone) => Ok(ConstructData::Alone),
            Written::Style(Style::Builder) => Ok(ConstructData::Builder),
            Written::Table(Table { builders }) => {
                let rates: Option<Vec<Num>> = builders
                    .iter()
                    .map(|rate| rate.to_num().filter(|&rate| rate > Num::ZERO))
                    .collect();
                match rates {
                    Some(rates) if !rates.is_empty() => Ok(ConstructData::Builders(rates)),
                    _ => Err(D::Error::custom(
                        "a builders table holds a positive rate for each count, one at least",
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use toml::de;

    use super::*;

    #[test]
    fn a_style_reads_as_its_name_or_its_table_of_positive_rates() {
        let read = |text: &str| -> Result<ConstructData, de::Error> {
            #[derive(Debug, Deserialize)]
            struct Field {
                construct: ConstructData,
            }
            toml::from_str::<Field>(text).map(|field| field.construct)
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
            r"construct = { builders = [] }",
            r#"construct = { builders = ["0"] }"#,
            r#"construct = { builders = ["-1"] }"#,
            r#"construct = { builders = ["1"], rest = 2 }"#,
        ] {
            assert!(read(flawed).is_err(), "{flawed}");
        }
    }
}
