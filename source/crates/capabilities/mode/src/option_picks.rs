use std::collections::BTreeMap;

use campfire_values::{DeclaredName, Scalar};

use crate::error::PickError;
use crate::option_data::OptionData;
use crate::option_value::OptionValue;

/// The value a session's host picked of each of the mode's options, by its place among the
/// option's values, in the order of the options' names (21-match-options.md, The host picks, and
/// the terms record the pick). Only its constructors make one, so each place is one of its
/// option's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionPicks(Box<[u8]>);

impl OptionPicks {
    /// The default of each of `options`.
    pub fn defaults(options: &BTreeMap<DeclaredName, OptionData>) -> Self {
        Self(options.values().map(OptionData::default_place).collect())
    }

    /// The default of each of `options` but those `values` pick, each by its value; an error
    /// for a pick of an option `options` lacks, of a value its option lacks, or of one option
    /// twice.
    pub fn named(
        options: &BTreeMap<DeclaredName, OptionData>,
        values: &[OptionValue],
    ) -> Result<Self, PickError> {
        let mut picks = Self::defaults(options);
        let mut picked = vec![false; options.len()];
        for OptionValue { name, value } in values {
            let at = options
                .keys()
                .position(|held| held == name)
                .ok_or_else(|| PickError::Unknown(name.clone()))?;
            if picked[at] {
                return Err(PickError::Twice(name.clone()));
            }
            picked[at] = true;
            picks.0[at] = options[name]
                .place(*value)
                .ok_or_else(|| PickError::NoValue {
                    option: name.clone(),
                    value: *value,
                })?;
        }
        Ok(picks)
    }

    /// `places`, as a session's terms carry them, the picks of `options`; an error for another
    /// count than the options', or a place past its option's values.
    pub fn checked(
        options: &BTreeMap<DeclaredName, OptionData>,
        places: &[u8],
    ) -> Result<Self, PickError> {
        if places.len() != options.len() {
            return Err(PickError::Count {
                given: places.len(),
                options: options.len(),
            });
        }
        for ((name, option), &place) in options.iter().zip(places) {
            if option.value(place).is_none() {
                return Err(PickError::Place {
                    option: name.clone(),
                    place,
                });
            }
        }
        Ok(Self(places.into()))
    }

    /// The places, as a session's terms carry them.
    pub fn places(&self) -> &[u8] {
        &self.0
    }

    /// Each of `options` by its name, with its picked value, in the order of their names;
    /// `options` are the ones it was made of.
    pub fn values<'a>(
        &'a self,
        options: &'a BTreeMap<DeclaredName, OptionData>,
    ) -> impl Iterator<Item = (&'a DeclaredName, Scalar)> {
        debug_assert_eq!(self.0.len(), options.len(), "picks of other options");
        options.iter().zip(&self.0).map(|((name, option), &place)| {
            let value = option
                .value(place)
                .expect("a pick is one of its option's values");
            (name, value)
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn picks_are_a_place_of_each_option_in_the_order_of_their_names() {
        let option = |text: &str| Toml::parse::<OptionData>(text).unwrap();
        let options: BTreeMap<DeclaredName, OptionData> = [
            (
                "money",
                option("values = [5000, 10000, 50000]\ndefault = 10000"),
            ),
            ("bounty", option("values = [3, 40]\ndefault = 40")),
        ]
        .map(|(name, option)| (DeclaredName::new(name).unwrap(), option))
        .into();
        // By name, the bounty first, then the money.
        let named = |picks: &OptionPicks| {
            let values = picks.values(&options);
            values
                .map(|(name, value)| (name.as_str().to_owned(), value))
                .collect::<Vec<_>>()
        };
        let defaults = OptionPicks::defaults(&options);
        assert_eq!(defaults.places(), [1, 1]);
        assert_eq!(
            named(&defaults),
            [
                ("bounty".to_owned(), Scalar::Int(40)),
                ("money".to_owned(), Scalar::Int(10000))
            ]
        );
        let rich = OptionPicks::checked(&options, &[0, 2]).unwrap();
        assert_eq!(
            named(&rich),
            [
                ("bounty".to_owned(), Scalar::Int(3)),
                ("money".to_owned(), Scalar::Int(50000))
            ]
        );
        // Another count than the options', and a place past an option's values, are refused.
        assert_eq!(
            OptionPicks::checked(&options, &[0]),
            Err(PickError::Count {
                given: 1,
                options: 2
            })
        );
        assert_eq!(
            OptionPicks::checked(&options, &[0, 3]),
            Err(PickError::Place {
                option: DeclaredName::new("money").unwrap(),
                place: 3
            })
        );
        // By value: the other options keep their defaults; an option the mode lacks, a value
        // its option lacks, and one option twice, are refused.
        let value = |text: &str| text.parse::<OptionValue>().unwrap();
        let rich = OptionPicks::named(&options, &[value("money=50000")]).unwrap();
        assert_eq!(rich.places(), [1, 2]);
        let both = [value("bounty=3"), value("money=5000")];
        assert_eq!(
            OptionPicks::named(&options, &both).unwrap().places(),
            [0, 0]
        );
        let refused = |values: &[OptionValue]| OptionPicks::named(&options, values).unwrap_err();
        let name = |name: &str| DeclaredName::new(name).unwrap();
        assert_eq!(
            refused(&[value("speed=2")]),
            PickError::Unknown(name("speed"))
        );
        assert_eq!(
            refused(&[value("money=7")]),
            PickError::NoValue {
                option: name("money"),
                value: Scalar::Int(7)
            }
        );
        assert_eq!(
            refused(&[value("money=5000"), value("money=10000")]),
            PickError::Twice(name("money"))
        );
        let none = BTreeMap::<DeclaredName, OptionData>::new();
        assert!(OptionPicks::defaults(&none).places().is_empty());
        assert!(OptionPicks::checked(&none, &[]).is_ok());
    }
}
