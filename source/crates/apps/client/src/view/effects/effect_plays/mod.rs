use std::collections::BTreeMap;

use campfire_client_files::ClientUnits;
use campfire_script::MatchNames;
use campfire_units::TypeOriginBook;
use campfire_values::{ActionId, PackageIndex, UnitType};

use crate::view::effects::effect_book::{EffectBook, ListId, SystemId};

/// What the match's unit types and actions play, of the mode's own package's looks, each name
/// resolved once to the book's list or system (effects.md, An effect is the client's, from the
/// state it holds): a type's death, and its projectile's end and trail; an action's fire and its
/// hit.
#[derive(Debug, Default)]
pub(crate) struct EffectPlays {
    pub(crate) deaths: BTreeMap<UnitType, ListId>,
    pub(crate) ends: BTreeMap<UnitType, ListId>,
    pub(crate) trails: BTreeMap<UnitType, SystemId>,
    pub(crate) fires: BTreeMap<ActionId, ListId>,
    pub(crate) hits: BTreeMap<ActionId, ListId>,
}

/// What the plays are of: the effects' book, the mode's own package's looks, the match's types'
/// origins and its names.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlaySources<'a> {
    pub(crate) book: &'a EffectBook,
    pub(crate) looks: &'a ClientUnits,
    pub(crate) origins: &'a TypeOriginBook,
    pub(crate) names: &'a MatchNames,
}

impl EffectPlays {
    /// The plays of `sources`' looks, of its types and its actions, in its book; a name the book
    /// does not hold plays nothing.
    pub(crate) fn of(sources: PlaySources<'_>) -> Self {
        let PlaySources {
            book,
            looks,
            origins,
            names,
        } = sources;
        let mut plays = Self::default();
        for (unit_type, origin) in origins.iter() {
            if origin.package != PackageIndex::MODE {
                continue;
            }
            let Some(look) = looks.units().get(&*origin.name) else {
                continue;
            };
            let list = |name: &Option<_>| book.list_id(name.as_ref()?);
            if let Some(death) = list(&look.death) {
                plays.deaths.insert(unit_type, death);
            }
            if let Some(end) = list(&look.end) {
                plays.ends.insert(unit_type, end);
            }
            if let Some(trail) = look.trail.as_ref().and_then(|name| book.id(name)) {
                plays.trails.insert(unit_type, trail);
            }
        }
        for (name, look) in looks.actions() {
            let Some(action) = names.named::<ActionId>((PackageIndex::MODE, name.as_str())) else {
                continue;
            };
            if let Some(fire) = look.fire.as_ref().and_then(|name| book.list_id(name)) {
                plays.fires.insert(action, fire);
            }
            if let Some(hit) = look.hit.as_ref().and_then(|name| book.list_id(name)) {
                plays.hits.insert(action, hit);
            }
        }
        plays
    }
}

#[cfg(test)]
mod tests;
