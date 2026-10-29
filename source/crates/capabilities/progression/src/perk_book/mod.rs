use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::DeclaredName;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_names::{MatchNames, UnknownName};
use campfire_stats::{Level, ModifierBook};
use campfire_values::{ModifierId, NameId, Named, Names, PackageIndex, PerkId, TrackId};

use crate::error::PerkError;
use crate::experience::Experience;
use crate::mode_perks_data::ModePerksData;
use crate::perk_data::PerkData;
use crate::track_book::TrackBook;
use crate::track_data::TrackHolder;

/// The perks a match loaded, by id (04-capabilities/progression.md, A science is a perk of a
/// player). Package data, not state: a restore loads it from the packages, as a new match does. A
/// clone shares the perks.
#[derive(Resource, Debug, Clone, Default)]
#[component(immutable)]
pub struct PerkBook(Arc<Perks>);

#[derive(Debug, Default)]
struct Perks {
    /// Shared with the script view.
    names: Arc<Names<PerkId>>,
    perks: Box<[Perk]>,
}

/// A perk, its names resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Perk {
    /// None for a perk no player learns, which the mode's script alone grants.
    cost: Option<NonZeroU32>,
    requires: Box<[PerkId]>,
    level: Option<PerkLevel>,
    modifier: ModifierId,
}

/// The level of a player track a perk needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PerkLevel {
    track: TrackId,
    level: Level,
}

/// What a learn of a perk reads of its player: the perks it has, in order, its points, none in a
/// mode whose player tracks give none, and its experience, none in a mode of no player track.
#[derive(Debug, Clone, Copy)]
pub struct Learner<'a> {
    pub perks: &'a [PerkId],
    pub points: Option<u32>,
    pub experience: Option<&'a Experience>,
}

impl PerkBook {
    /// The perks' names.
    pub fn names(&self) -> &Arc<Names<PerkId>> {
        &self.0.names
    }

    /// Whether the mode has no perk.
    pub fn is_empty(&self) -> bool {
        self.0.perks.is_empty()
    }

    /// The player modifier a player holds once it has `perk`.
    pub fn modifier(&self, perk: PerkId) -> ModifierId {
        self.perk(perk).modifier
    }

    /// What a learn of `perk` costs, the points the player spends: none for a perk no player
    /// learns, when `held` holds it already, its level on the perk's track is short of the one it
    /// needs, it lacks one of the perks it requires, or its points are short of the cost.
    pub fn learn_cost(&self, perk: PerkId, held: Learner<'_>) -> Option<u32> {
        let at = self.perk(perk);
        let cost = at.cost?.get();
        let has = |perk| held.perks.binary_search(&perk).is_ok();
        let reached = at.level.is_none_or(|needed| {
            let level = held
                .experience
                .and_then(|experience| experience.get(needed.track));
            level.is_some_and(|held| held.level >= needed.level)
        });
        let required = at.requires.iter().all(|&required| has(required));
        let afforded = held.points.is_some_and(|points| points >= cost);
        (!has(perk) && reached && required && afforded).then_some(cost)
    }

    fn perk(&self, perk: PerkId) -> &Perk {
        &self.0.perks[perk.index()]
    }

    /// The mode's `[perks]`, of its tracks and modifiers, as `new` resolves them.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let Some(perks) = cx.mode_section::<ModePerksData>() else {
            return Ok(Self::default());
        };
        let tracks = cx.book::<TrackBook>()?;
        let modifiers = cx.book::<ModifierBook>()?;
        Self::new(&perks.0, tracks, modifiers.names())
    }

    /// The book of `perks`, their names resolved in `tracks` and the mode's `modifiers`; an error
    /// for more than `PerkId::LIMIT`, a perk, a track or a modifier the mode does not have, a
    /// `track` with no `level` or a `level` with no `track`, a level of a unit's track or past its
    /// track's, and a cost in a mode whose player tracks give no point.
    fn new(
        perks: &BTreeMap<DeclaredName, PerkData>,
        tracks: &TrackBook,
        modifiers: &Names<ModifierId>,
    ) -> Result<Self, BookError> {
        if perks.len() > PerkId::LIMIT {
            let at = BookPlace::Section(ModePerksData::KEY);
            return Err(BookError::rule(PackageIndex::MODE, at, PerkError::TooMany));
        }
        let mut names = Names::default();
        for name in perks.keys() {
            names.add(name.as_str());
        }
        let paid = tracks
            .player_tracks()
            .iter()
            .any(|track| tracks.gives_points(track));
        let built = perks.iter().map(|(name, data)| {
            let entry = || BookPlace::Entry {
                table: ModePerksData::KEY,
                name: name.clone(),
            };
            let rule = |error| BookError::rule(PackageIndex::MODE, entry(), error);
            let unknown = |kind, name: &DeclaredName| {
                BookError::unknown(PackageIndex::MODE, entry(), UnknownName::new(kind, name))
            };
            if data.cost.is_some() && !paid {
                return Err(rule(PerkError::NoPoints));
            }
            let requires = data.requires.iter().map(|required| {
                names
                    .named(required.as_str())
                    .ok_or_else(|| unknown(PerkId::NAMES, required))
            });
            let level = match (&data.track, data.level) {
                (None, None) => None,
                (Some(track), Some(level)) => {
                    let id = tracks
                        .names()
                        .named(track.as_str())
                        .ok_or_else(|| unknown(TrackId::NAMES, track))?;
                    if tracks.holder(id) != TrackHolder::Player {
                        return Err(rule(PerkError::UnitTrack(track.clone())));
                    }
                    let levels = tracks.levels(id);
                    if usize::try_from(level.get()).is_ok_and(|level| level > levels) {
                        return Err(rule(PerkError::Level {
                            level: level.get(),
                            levels,
                        }));
                    }
                    Some(PerkLevel { track: id, level })
                }
                _ => return Err(rule(PerkError::LevelTrack)),
            };
            let modifier = modifiers
                .named((PackageIndex::MODE, data.modifier.as_str()))
                .ok_or_else(|| unknown(ModifierId::NAMES, &data.modifier))?;
            Ok(Perk {
                cost: data.cost,
                requires: requires.collect::<Result<_, _>>()?,
                level,
                modifier,
            })
        });
        Ok(Self(Arc::new(Perks {
            perks: built.collect::<Result<_, _>>()?,
            names: Arc::new(names),
        })))
    }
}

impl Book for PerkBook {
    const SCRIPTS_ONLY: bool = false;

    /// Names the perks to scripts.
    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(self.names()));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use campfire_common::DeclaredName;
    use campfire_values::{ModifierId, Names};

    use crate::perk_book::PerkBook;
    use crate::perk_data::PerkData;
    use crate::track_book::TrackBook;

    impl PerkBook {
        /// The book of a test's `perks`, of `tracks` and the mode's `modifiers`, which hold
        /// every name the perks give.
        pub fn of(
            perks: &BTreeMap<DeclaredName, PerkData>,
            tracks: &TrackBook,
            modifiers: &Names<ModifierId>,
        ) -> Self {
            Self::new(perks, tracks, modifiers).expect("a test's perks load")
        }
    }
}

#[cfg(test)]
mod tests;
