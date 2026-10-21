use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_values::Scalar;

use crate::error::PickError;
use crate::option_data::OptionData;
use crate::option_value::OptionValue;

/// The value a session's host picked of each of the mode's options, by its place among the
/// option's values, in the order of the options' names (08-sessions.md, The host picks, and
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
mod tests;
