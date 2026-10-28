use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Changed;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_script::{MatchNames, UnknownName};
use campfire_sim::StableId;
use campfire_stats::{Grant, GrantFrom, Grants, Hold, Level, ModifierBook, Modifiers};
use campfire_values::{ModifierId, NameId, Named, PackageIndex, Rank, TrackId};

use crate::experience::Experience;
use crate::mode_tracks_data::ModeTracksData;
use crate::track_book::TrackBook;

/// The modifier each track's units hold at each of its levels from 2, by track, none for a track of
/// no `modifiers` (04-capabilities/progression.md, A level holds a modifier). Package data, not
/// state.
#[derive(Resource, Debug, Clone, Default)]
#[component(immutable)]
pub struct TrackModifierBook(Arc<[Box<[ModifierId]>]>);

impl TrackModifierBook {
    /// The modifiers of each track, in the order of their ids.
    pub(crate) const fn new(modifiers: Arc<[Box<[ModifierId]>]>) -> Self {
        Self(modifiers)
    }

    /// The modifier a unit at `level` on `track` holds; none at level 1 and on a track of none.
    pub fn of(&self, track: TrackId, level: Level) -> Option<ModifierId> {
        let at = usize::try_from(level.get()).expect("a level fits usize");
        let modifiers = self.0.get(track.index())?;
        at.checked_sub(2).and_then(|at| modifiers.get(at)).copied()
    }

    /// Whether any track gives a modifier.
    pub fn any(&self) -> bool {
        self.0.iter().any(|modifiers| !modifiers.is_empty())
    }

    /// The modifiers of the mode's `[tracks]`, of the mode's package, in the order of the
    /// tracks' ids; an error for a modifier the mode does not have. The track book checks their
    /// counts.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let Some(tracks) = cx.mode_section::<ModeTracksData>() else {
            return Ok(Self::default());
        };
        cx.book::<TrackBook>()?;
        let book = cx.book::<ModifierBook>()?;
        let at = || BookPlace::Section(ModeTracksData::KEY);
        let modifiers = tracks.0.values().map(|track| {
            let named = track.modifiers.iter().map(|name| {
                let named = book.names().named((PackageIndex::MODE, name.as_str()));
                named.ok_or_else(|| {
                    let unknown = UnknownName::new(ModifierId::NAMES, name);
                    BookError::unknown(PackageIndex::MODE, at(), unknown)
                })
            });
            named.collect::<Result<Box<[ModifierId]>, BookError>>()
        });
        Ok(Self::new(modifiers.collect::<Result<_, _>>()?))
    }

    /// Pushes the grants of each unit whose experience changed that differ from what it holds
    /// under a level's hold: the modifier of its level on each of its tracks that gives one,
    /// from itself at rank 1, and a release of each other it holds so, as a passive is held
    /// (04-capabilities/stats.md, Passives by one pass).
    pub(crate) fn want(
        book: Res<'_, Self>,
        mut grants: ResMut<'_, Grants>,
        units: Query<
            '_,
            '_,
            (Entity, &StableId, &Experience, Option<&Modifiers>),
            Changed<Experience>,
        >,
    ) {
        for (entity, &id, experience, modifiers) in &units {
            let wanted = || {
                experience
                    .iter()
                    .filter_map(|held| book.of(held.track, held.level))
            };
            let held = || {
                modifiers
                    .into_iter()
                    .flat_map(Modifiers::iter)
                    .filter(|carried| {
                        let instance = carried.instance;
                        instance.source == Some(id) && instance.lifetime.held_by(Hold::Level)
                    })
            };
            let grant = |modifier, rank| Grant {
                modifier,
                from: GrantFrom::Itself,
                hold: Hold::Level,
                action: None,
                rank,
            };
            let mut run = grants.of(entity, id);
            for carried in held() {
                let modifier = carried.instance.id;
                if !wanted().any(|wanted| wanted == modifier) {
                    run.push(grant(modifier, None));
                }
            }
            for modifier in wanted() {
                if !held().any(|carried| carried.instance.id == modifier) {
                    run.push(grant(modifier, Some(Rank::FIRST)));
                }
            }
        }
    }
}

impl Book for TrackModifierBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}
