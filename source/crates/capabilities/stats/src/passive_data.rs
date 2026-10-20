use std::collections::BTreeMap;

use campfire_actions::ActionsData;
use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, Section, SectionTable};
use serde::Deserialize;

use crate::error::AppliedError;

/// A unit type's `passive`: a modifier of its package it holds from its spawn, from itself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct PassiveData(pub DeclaredName);

impl Section for PassiveData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "passive";
}

impl PassiveData {
    /// The rule of the passives: a modifier is the passive of one holder of its package at most,
    /// of its unit types by `passive` and its actions by `passive_modifier`, as two would share
    /// its hold.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let types = cx.types().iter().filter(|unit| unit.package == package);
            let types = types.filter_map(|unit| {
                let passive = unit.data.section::<Self>()?;
                Some((BookPlace::UnitType(unit.place()), &passive.0))
            });
            let actions = source.content.get::<ActionsData>();
            let actions = actions.into_iter().flat_map(|actions| &actions.0);
            let actions = actions.filter_map(|(id, action)| {
                let at = BookPlace::Entry {
                    table: ActionsData::KEY,
                    name: id.clone(),
                };
                Some((at, action.passive_modifier.as_ref()?))
            });
            let mut held: BTreeMap<&DeclaredName, BookPlace> = BTreeMap::new();
            for (holder, modifier) in types.chain(actions) {
                if let Some(first) = held.get(modifier) {
                    let problem = AppliedError::SharedPassive {
                        modifier: modifier.clone(),
                        first: first.clone(),
                    };
                    return Err(BookError::rule(package, holder, problem));
                }
                held.insert(modifier, holder);
            }
        }
        Ok(())
    }
}
