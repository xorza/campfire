use std::ops::ControlFlow;

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Local, Query, Res, SystemState};
use bevy_ecs::world::World;
use campfire_actions::Targets;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, ById, ByIdBuilder, Section};
use campfire_geometry::Placed;
use campfire_math::{Num, Rounding};
use campfire_script::{Filter, MatchNames, UnknownName};
use campfire_sim::{EntityIndex, Position, StableId};
use campfire_stats::Level;
use campfire_units::{Deaths, Owner, UnitTypes, UnitsNear};
use campfire_values::{Named, PackageIndex, Team, TrackId, UnitTags, UnitType};

use crate::error::ExperienceError;
use crate::experience::Experience;
use crate::experience_data::{ExperienceData, ExperienceTo, Gives, ModeExperienceData};
use crate::level_ups::LevelUps;
use crate::track_book::TrackBook;
use crate::track_data::TrackHolder;

/// How the mode's deaths give experience, as its `[[experience]]` shares and its unit types'
/// `gives` give it; none for a mode of no shares. Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct ExperienceBook {
    shares: Box<[ExperienceShare]>,
    gives: ById<UnitType, Gives>,
}

/// A share of a death's experience: the track it goes to, to whom, and its victims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExperienceShare {
    track: TrackId,
    to: To,
    victim: Filter,
}

/// To whom a share goes, its filters resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum To {
    Killer { killer: Filter },
    Player { killer: Filter },
    Near { radius: Num, filter: Filter },
}

/// Experience a death gives a unit, on a share's track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExperienceGrant {
    entity: Entity,
    id: StableId,
    track: TrackId,
    amount: Num,
}

/// Experience a death gives a player, on a share's track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerGrant {
    player: Entity,
    track: TrackId,
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
            Option<&'static Level>,
            Option<&'static UnitTags>,
            Option<&'static Experience>,
        ),
    >,
    Query<'static, 'static, &'static Owner>,
);

impl ExperienceBook {
    /// The shares of the mode's `[[experience]]` and its types' `gives`, their names resolved;
    /// an error for a track, or a filter's tag, the match does not have, for a list of experience
    /// of another count than the `level` track's levels, as it reads the victim's level, or in a
    /// mode of none, and for a type's experience in a mode of no share.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let mode = cx.mode_section::<ModeExperienceData>();
        let book = cx.book::<TrackBook>()?;
        let at = || BookPlace::Section(ModeExperienceData::KEY);
        let shares = mode.map_or(&[][..], |mode| &mode.0);
        let tags = cx.fact::<UnitTypes>()?.tags();
        let resolve = |filter| {
            Filter::resolve(filter, tags)
                .map_err(|name| BookError::unknown(PackageIndex::MODE, at(), name))
        };
        let shares = shares
            .iter()
            .map(|share| {
                let track = book.names().named(share.track.as_str()).ok_or_else(|| {
                    let name = UnknownName::new(TrackId::NAMES, &share.track);
                    BookError::unknown(PackageIndex::MODE, at(), name)
                })?;
                let to = match &share.to {
                    ExperienceTo::Killer { killer } => To::Killer {
                        killer: resolve(killer)?,
                    },
                    ExperienceTo::Player { killer } => To::Player {
                        killer: resolve(killer)?,
                    },
                    ExperienceTo::Near { radius, filter } => To::Near {
                        radius: *radius,
                        filter: resolve(filter)?,
                    },
                };
                let to_player = matches!(to, To::Player { .. });
                if to_player != (book.holder(track) == TrackHolder::Player) {
                    let error = ExperienceError::ShareHolder;
                    return Err(BookError::rule(PackageIndex::MODE, at(), error));
                }
                Ok(ExperienceShare {
                    track,
                    to,
                    victim: resolve(share.victim.filter())?,
                })
            })
            .collect::<Result<Box<[_]>, BookError>>()?;
        let mut gives = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<ExperienceData>() else {
                continue;
            };
            let place = || BookPlace::UnitType(unit.place());
            if shares.is_empty() {
                let error = ExperienceError::NoModeExperience;
                return Err(BookError::rule(unit.package, place(), error));
            }
            if let Gives::ByLevel(values) = &data.gives {
                let Some(track) = book.level_track() else {
                    let error = ExperienceError::NoLevelTrack;
                    return Err(BookError::rule(unit.package, place(), error));
                };
                let levels = book.levels(track);
                if values.len() != levels {
                    let error = ExperienceError::Levels {
                        given: values.len(),
                        levels,
                    };
                    return Err(BookError::rule(unit.package, place(), error));
                }
            }
            gives.set(unit.unit_type, data.gives.clone());
        }
        Ok(Self {
            shares,
            gives: gives.build(),
        })
    }

    /// Whether the mode shares a death's experience.
    pub(crate) fn shares(&self) -> bool {
        !self.shares.is_empty()
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

impl ExperienceShare {
    /// Gives each of the tick's deaths of a type of experience, at the victim's level, on each
    /// share whose victim filter its tags at death select, to the units of the share's track: to
    /// its killer, when the killer passes the share's filter from the victim's side, or split
    /// evenly, rounded to the nearest, among the living units the filter selects for the victim
    /// whose bodies come within the radius of its point, in the order of their stable ids, as a
    /// script's `ctx.find` finds them. Each raises its unit's level as a script's `add_xp` does
    /// (04-capabilities/games.md, A death's experience; generals.md, A death's experience goes by
    /// shares).
    pub(crate) fn give(
        world: &mut World,
        mut reads: Local<'_, SystemState<ShareReads>>,
        mut grants: Local<'_, Vec<ExperienceGrant>>,
        mut near: Local<'_, Vec<ExperienceGrant>>,
        mut players: Local<'_, Vec<PlayerGrant>>,
    ) {
        grants.clear();
        players.clear();
        {
            let reads = reads.get(world);
            let (deaths, book, index, units_near, targets, units, owners) =
                reads.expect("a match that shares experience holds what the share reads");
            for death in &deaths {
                let Some(victim) = index.get(death.fallen.id) else {
                    continue;
                };
                let Ok((&position, unit_type, team, level, _, _)) = units.get(victim) else {
                    continue;
                };
                let gives = unit_type.and_then(|&unit_type| book.gives.get(unit_type));
                let level = level.map_or(1, |level| level.get());
                let (Some(gives), Some(&team)) = (gives, team) else {
                    continue;
                };
                let Some(amount) = gives.at(level) else {
                    continue;
                };
                for share in &book.shares {
                    if !share.victim.selects_tags(death.fallen.tags) {
                        continue;
                    }
                    let holds = |entity: Entity| {
                        let experience = units
                            .get(entity)
                            .ok()
                            .and_then(|(.., experience)| experience);
                        experience.is_some_and(|experience| experience.get(share.track).is_some())
                    };
                    let passes = |entity: Entity, killer: Filter| {
                        units.get(entity).is_ok_and(|(_, _, other, _, tags, _)| {
                            let tags = tags.map_or_else(Default::default, |tags| tags.tags);
                            other.is_some_and(|&other| {
                                killer.selects(targets.relation(team, other), tags)
                            })
                        })
                    };
                    match share.to {
                        To::Killer { killer } => {
                            let found =
                                death.killer.and_then(|id| index.get(id).map(|at| (id, at)));
                            let Some((id, entity)) = found.filter(|&(_, entity)| holds(entity))
                            else {
                                continue;
                            };
                            if passes(entity, killer) {
                                grants.push(ExperienceGrant {
                                    entity,
                                    id,
                                    track: share.track,
                                    amount,
                                });
                            }
                        }
                        To::Player { killer } => {
                            let found = death.killer.and_then(|id| index.get(id));
                            let Some(entity) = found.filter(|&entity| passes(entity, killer))
                            else {
                                continue;
                            };
                            let player = owners.get(entity).ok().and_then(|owner| {
                                index.get(StableId::of_player(owner.player_slot()))
                            });
                            if let Some(player) = player {
                                players.push(PlayerGrant {
                                    player,
                                    track: share.track,
                                    amount,
                                });
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
                                        near.push(ExperienceGrant {
                                            entity: body.key,
                                            id: unit.id,
                                            track: share.track,
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
                            grants.extend(near.iter().map(|grant| ExperienceGrant {
                                amount: each,
                                ..*grant
                            }));
                        }
                    }
                }
            }
        }
        for &ExperienceGrant {
            entity,
            id,
            track,
            amount,
        } in &*grants
        {
            LevelUps::grant(world, entity, id, track, amount);
        }
        for &PlayerGrant {
            player,
            track,
            amount,
        } in &*players
        {
            LevelUps::grant_player(world, player, track, amount);
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
