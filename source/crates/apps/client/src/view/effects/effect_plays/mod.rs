use std::collections::BTreeMap;

use campfire_client_files::ClientUnits;
use campfire_script::MatchNames;
use campfire_units::TypeOriginBook;
use campfire_values::{ActionId, PackageIndex, PackagePath, UnitType};

use crate::view::effects::chassis_run::ChassisPlay;
use crate::view::effects::effect_book::{EffectBook, ListId, SoundId, SystemId};
use crate::view::effects::fall_run::FallPlay;
use crate::view::effects::wreck_run::WreckPlay;
use crate::view::float_num::FloatNum;

/// What the match's unit types and actions play, of the mode's own package's looks, each name
/// resolved once to the book's list or system (02-engine-core.md, An effect is the client's, from
/// the state it holds): a type's death and its fall, its projectile's end and trail, its treads and
/// its chassis; an action's fire and its hit; and the textures of the tracks, each once, by their
/// groups.
#[derive(Debug, Default)]
pub(crate) struct EffectPlays {
    pub(crate) deaths: BTreeMap<UnitType, DeathPlays>,
    pub(crate) falls: BTreeMap<UnitType, FallPlay>,
    pub(crate) treads: BTreeMap<UnitType, TreadPlay>,
    pub(crate) chassis: BTreeMap<UnitType, ChassisPlay>,
    pub(crate) track_textures: Vec<PackagePath>,
    pub(crate) ends: BTreeMap<UnitType, ListId>,
    pub(crate) trails: BTreeMap<UnitType, SystemId>,
    pub(crate) fires: BTreeMap<ActionId, ListId>,
    pub(crate) hits: BTreeMap<ActionId, ListId>,
    pub(crate) fire_sounds: BTreeMap<ActionId, SoundId>,
}

/// The lists a type's death plays, of those the book holds: as its unit dies, at the middle of its
/// keep, and as it goes (02-engine-core.md, A death plays its phases); and the wrecks each of those
/// phases throws, of each one list drawn at random (02-engine-core.md, A death names its wrecks by
/// phase).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DeathPlays {
    pub(crate) start: Vec<ListId>,
    pub(crate) middle: Vec<ListId>,
    pub(crate) end: Vec<ListId>,
    pub(crate) wrecks: [Vec<Vec<WreckPlay>>; 3],
}

/// What a type's treads play (02-engine-core.md, A tread throws dust while it drives;
/// 02-engine-core.md, A tread lays track marks): their systems, of those the book holds, and their
/// track.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TreadPlay {
    pub(crate) systems: Vec<SystemId>,
    pub(crate) track: Option<TrackPlay>,
}

/// A tread's track: its texture's group, and its width in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrackPlay {
    pub(crate) group: usize,
    pub(crate) width: f32,
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
            let lists = |names: &[_]| names.iter().filter_map(|name| book.list_id(name)).collect();
            let throws = |lists| WreckPlay::lists(lists, PackageIndex::MODE);
            let wrecks = &look.death.wrecks;
            let death = DeathPlays {
                start: lists(&look.death.start),
                middle: lists(&look.death.middle),
                end: lists(&look.death.end),
                wrecks: [
                    throws(&wrecks.start),
                    throws(&wrecks.middle),
                    throws(&wrecks.end),
                ],
            };
            if death != DeathPlays::default() {
                plays.deaths.insert(unit_type, death);
            }
            if let Some(fall) = &look.death.fall {
                plays.falls.insert(unit_type, FallPlay::of(fall, book));
            }
            if let Some(end) = list(&look.end) {
                plays.ends.insert(unit_type, end);
            }
            if let Some(trail) = look.trail.as_ref().and_then(|name| book.id(name)) {
                plays.trails.insert(unit_type, trail);
            }
            let tread = TreadPlay {
                systems: (look.tread.systems.iter())
                    .filter_map(|name| book.id(name))
                    .collect(),
                track: look.tread.track.as_ref().map(|track| {
                    let textures = &mut plays.track_textures;
                    let group = textures
                        .iter()
                        .position(|texture| *texture == track.texture);
                    let group = group.unwrap_or_else(|| {
                        textures.push(track.texture.clone());
                        textures.len() - 1
                    });
                    TrackPlay {
                        group,
                        width: track.width.float(),
                    }
                }),
            };
            if tread != TreadPlay::default() {
                plays.treads.insert(unit_type, tread);
            }
            if let Some(chassis) = &look.chassis {
                plays.chassis.insert(unit_type, ChassisPlay::of(chassis));
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
            if let Some(sound) = look
                .fire_sound
                .as_ref()
                .and_then(|name| book.sound_id(name))
            {
                plays.fire_sounds.insert(action, sound);
            }
        }
        plays
    }
}

#[cfg(test)]
mod tests;
