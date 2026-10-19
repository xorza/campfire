use std::ops::ControlFlow;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Local, Query, Res, SystemState};
use bevy_ecs::world::World;
use campfire_actions::Targets;
use campfire_declare::{Book, BookCx, BookError, BookPlace, ByType, Section};
use campfire_geometry::Placed;
use campfire_math::{Num, Rounding};
use campfire_script::{Filter, MatchNames, UnknownName};
use campfire_sim::{EntityIndex, Position, StableId};
use campfire_units::{Deaths, UnitTypes, UnitsNear};
use campfire_values::{Named, PackageIndex, Relation, Team, TrackId, UnitType};

use crate::error::ExperienceError;
use crate::experience::Experience;
use crate::experience_data::{ExperienceData, ExperienceTo, Gives, ModeExperienceData};
use crate::level_ups::LevelUps;
use crate::track_book::TrackBook;

/// How the mode's deaths give experience, as its `[experience]` and its unit types' give it; none
/// for a mode of no `[experience]`. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ExperienceBook(Option<Share>);

/// The track a death's experience goes to, to whom, and what each type's death gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Share {
    track: TrackId,
    to: To,
    gives: ByType<Gives>,
}

/// To whom a death's experience goes, its filter resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum To {
    Killer,
    Near { radius: Num, filter: Filter },
}

/// Experience a death gives a unit, on the share's track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Grant {
    entity: Entity,
    id: StableId,
    amount: Num,
}

/// What the share reads of the match.
type ShareReads = (
    Res<'static, Deaths>,
    Res<'static, ExperienceBook>,
    Res<'static, EntityIndex>,
    UnitsNear<'static>,
    Targets<'static, 'static>,
    Query<
        'static,
        'static,
        (
            &'static Position,
            Option<&'static UnitType>,
            Option<&'static Team>,
            Option<&'static Experience>,
        ),
    >,
);

impl ExperienceBook {
    /// The share of the mode's `[experience]` and its types', their names resolved; an error for
    /// a track, or a filter's tag, the match does not have, for a list of experience of another
    /// count than the track's levels, and for a type's experience in a mode of none.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mode = cx.mode_section::<ModeExperienceData>();
        let book = cx.book::<TrackBook>()?;
        let at = || BookPlace::Section(ModeExperienceData::KEY);
        let track = match mode {
            Some(mode) => Some(book.names().named(mode.track.as_str()).ok_or_else(|| {
                let name = UnknownName::new(TrackId::NAMES, &mode.track);
                BookError::unknown(PackageIndex::MODE, at(), name)
            })?),
            None => None,
        };
        let mut gives = ByType::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<ExperienceData>() else {
                continue;
            };
            let place = BookPlace::UnitType(unit.place());
            let Some(track) = track else {
                return Err(BookError::rule(
                    unit.package,
                    place,
                    ExperienceError::NoModeExperience,
                ));
            };
            if let Gives::ByLevel(values) = &data.gives {
                let levels = book.levels(track);
                if values.len() != levels {
                    let error = ExperienceError::Levels {
                        given: values.len(),
                        levels,
                    };
                    return Err(BookError::rule(unit.package, place, error));
                }
            }
            gives.set(unit.unit_type, data.gives.clone());
        }
        let (Some(mode), Some(track)) = (mode, track) else {
            return Ok(Self(None));
        };
        let to = match &mode.to {
            ExperienceTo::Killer => To::Killer,
            ExperienceTo::Near { radius, filter } => {
                let tags = cx.fact::<UnitTypes>()?.tags();
                let filter = Filter::resolve(filter, tags)
                    .map_err(|name| BookError::unknown(PackageIndex::MODE, at(), name))?;
                To::Near {
                    radius: *radius,
                    filter,
                }
            }
        };
        Ok(Self(Some(Share { track, to, gives })))
    }

    pub(crate) const fn share(&self) -> Option<&Share> {
        self.0.as_ref()
    }
}

impl Book for ExperienceBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

impl Gives {
    /// What it gives for a victim at `level` on the track, which a list holds, as the load
    /// checked; none past what a number holds.
    fn at(&self, level: u32) -> Option<Num> {
        match self {
            Self::Value(value) => Some(*value),
            Self::ByLevel(values) => {
                let at = usize::try_from(level - 1).expect("a level fits usize");
                Some(values[at])
            }
            Self::Rising { base, per_level } => {
                let rise = per_level.checked_mul_int(i64::from(level))?;
                base.checked_add(rise)
            }
        }
    }
}

impl Share {
    /// Gives each of the tick's deaths of a type of experience its experience on the track: to
    /// its killer, when it has the track and is no ally of the victim's, or split evenly, rounded
    /// to the nearest, among the living units the filter selects for the victim whose bodies come
    /// within the radius of its point and that have the track, in the order of their stable ids,
    /// as a script's `ctx.find` finds them. Each raises its unit's level as a script's `add_xp`
    /// does (04-capabilities/games.md, A death's experience).
    pub(crate) fn give(
        world: &mut World,
        mut reads: Local<'_, SystemState<ShareReads>>,
        mut grants: Local<'_, Vec<Grant>>,
        mut near: Local<'_, Vec<Grant>>,
    ) {
        grants.clear();
        let track = {
            let reads = reads.get(world);
            let (deaths, book, index, units_near, targets, units) =
                reads.expect("a match that shares experience holds what the share reads");
            let share = book.share().expect("the pass installs with a share");
            for death in deaths.iter() {
                let Some(victim) = index.get(death.fallen.id) else {
                    continue;
                };
                let Ok((&position, unit_type, team, experience)) = units.get(victim) else {
                    continue;
                };
                let gives = unit_type.and_then(|&unit_type| share.gives.get(unit_type));
                let level = experience
                    .and_then(|experience| experience.get(share.track))
                    .map_or(1, |held| held.level.get());
                let (Some(gives), Some(&team)) = (gives, team) else {
                    continue;
                };
                let Some(amount) = gives.at(level) else {
                    continue;
                };
                let holds = |entity: Entity| {
                    let experience = units
                        .get(entity)
                        .ok()
                        .and_then(|(.., experience)| experience);
                    experience.is_some_and(|experience| experience.get(share.track).is_some())
                };
                match share.to {
                    To::Killer => {
                        let killer = death.killer.and_then(|id| index.get(id).map(|at| (id, at)));
                        let Some((id, entity)) = killer.filter(|&(_, entity)| holds(entity)) else {
                            continue;
                        };
                        let ally = units.get(entity).ok().and_then(|(_, _, team, _)| team);
                        let ally = ally.is_some_and(|&other| {
                            targets.relation(team, other) == Relation::Friendly
                        });
                        if !ally {
                            grants.push(Grant { entity, id, amount });
                        }
                    }
                    To::Near { radius, filter } => {
                        near.clear();
                        let from = Placed::point(position.get());
                        units_near
                            .current()
                            .visit_near(position.get(), radius, |body| {
                                let Some(unit) = targets.target_at(body.key) else {
                                    return ControlFlow::Continue(());
                                };
                                let relation = targets.relation(team, unit.team);
                                if targets.reaches(from, radius, &unit)
                                    && filter.selects(relation, unit.tags)
                                    && holds(body.key)
                                {
                                    near.push(Grant {
                                        entity: body.key,
                                        id: unit.id,
                                        amount,
                                    });
                                }
                                ControlFlow::Continue(())
                            });
                        let Ok(count) = i64::try_from(near.len()) else {
                            continue;
                        };
                        let Some(each) = Num::from_int(count)
                            .and_then(|count| amount.checked_div(count, Rounding::NearestEven))
                        else {
                            continue;
                        };
                        near.sort_unstable_by_key(|grant| grant.id);
                        grants.extend(near.iter().map(|grant| Grant {
                            amount: each,
                            ..*grant
                        }));
                    }
                }
            }
            share.track
        };
        for &Grant { entity, id, amount } in &*grants {
            LevelUps::grant(world, entity, id, track, amount);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_death_gives_its_value_its_levels_entry_or_its_base_and_a_rise_for_each_level() {
        let ranks = Gives::ByLevel([20, 40, 60, 80].map(Num::int).to_vec());
        let rising = Gives::Rising {
            base: Num::int(150),
            per_level: Num::int(25),
        };
        let at = |gives: &Gives| [1, 2, 4].map(|level| gives.at(level));
        assert_eq!(at(&Gives::Value(Num::int(59))), [Some(Num::int(59)); 3]);
        assert_eq!(at(&ranks), [20, 40, 80].map(|xp| Some(Num::int(xp))));
        // 150 + 25 · 1, 150 + 25 · 2, 150 + 25 · 4.
        assert_eq!(at(&rising), [175, 200, 250].map(|xp| Some(Num::int(xp))));
        // A rise past what a number holds gives none.
        let past = Gives::Rising {
            base: Num::MAX,
            per_level: Num::ONE,
        };
        assert_eq!(past.at(1), None);
    }
}
