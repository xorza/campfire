use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_combat::DamagePass;
use campfire_common::Seat;
use campfire_declare::{Book, BookCx, BookError, BookPlace, Section};
use campfire_players::{HeldOut, PlayersWorld};
use campfire_script::{MatchNames, UnknownName};
use campfire_sim::{EntityIndex, SimTick, StableId};
use campfire_units::{Dead, Deaths, Owner, UnitTypes, UnitsWorld};
use campfire_values::{Named, PackageIndex, Tag, Team, UnitTags};

use crate::match_end::{MatchEnd, MatchResult};
use crate::mode_world::ModeWorld;
use crate::victory_data::{Defeat, VictoryData};

/// The mode's victory by the last unit of a tag, as its `[victory]` gives it; none for a mode of
/// no `[victory]`. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VictoryBook(Option<Victory>);

/// The tag of the units by which a player stays in play, and what its defeat does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Victory {
    counts: Tag,
    defeat: Defeat,
}

/// What the pass reads of a living unit: its id, its tags, its owner and its team.
type LivingUnit = (
    &'static StableId,
    &'static UnitTags,
    &'static Owner,
    &'static Team,
);

/// A player that holds a living unit of the tag, and its team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Holding {
    seat: Seat,
    team: Team,
}

impl VictoryBook {
    /// The victory of the mode's `[victory]`, its tag resolved; an error for a tag the match does
    /// not declare.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let Some(data) = cx.mode_section::<VictoryData>() else {
            return Ok(Self(None));
        };
        let tags = cx.fact::<UnitTypes>()?.tags();
        let counts = tags.named(data.counts.as_str()).ok_or_else(|| {
            let at = BookPlace::Section(VictoryData::KEY);
            BookError::unknown(
                PackageIndex::MODE,
                at,
                UnknownName::new(Tag::NAMES, &data.counts),
            )
        })?;
        Ok(Self(Some(Victory {
            counts,
            defeat: data.defeat,
        })))
    }

    pub(crate) const fn victory(self) -> Option<Victory> {
        self.0
    }
}

impl Book for VictoryBook {
    const SCRIPTS_ONLY: bool = true;

    fn share(&self, _: &mut MatchNames) {}
}

impl Victory {
    /// Follows the tick's deaths of owned units of the tag that counts: a player whose last
    /// living one died is out of play and, as its defeat says, its other units die as the next
    /// tick's deaths find them; then, of a match that runs, the one team whose players hold
    /// living units of the tag wins, and of none the match is a draw. The tick's deaths are
    /// followed together, so each finds the same units left (04-capabilities/games.md, Victory by
    /// the last unit of a tag).
    pub(crate) fn follow(
        world: &mut World,
        mut fallen: Local<'_, Vec<Seat>>,
        mut holding: Local<'_, Vec<Holding>>,
        mut doomed: Local<'_, Vec<StableId>>,
        mut units: Local<'_, QueryState<LivingUnit, Without<Dead>>>,
    ) {
        let victory = world.resource::<VictoryBook>().victory();
        let Self { counts, defeat } = victory.expect("the pass installs with a victory");
        fallen.clear();
        let index = world.resource::<EntityIndex>();
        for death in world.resource::<Deaths>().iter() {
            let Some(owner) = death.fallen.owner else {
                continue;
            };
            let entity = index.get(death.fallen.id);
            let tags = entity.and_then(|entity| world.get::<UnitTags>(entity));
            if tags.is_some_and(|tags| tags.tags.contains(counts)) && !fallen.contains(&owner) {
                fallen.push(owner);
            }
        }
        if fallen.is_empty() {
            return;
        }
        holding.clear();
        for (_, tags, owner, &team) in units.iter(world) {
            let held = Holding {
                seat: owner.seat(),
                team,
            };
            if tags.tags.contains(counts) && !holding.contains(&held) {
                holding.push(held);
            }
        }
        for &seat in &*fallen {
            if holding.iter().any(|held| held.seat == seat) {
                continue;
            }
            let player = world
                .player_entity(seat)
                .expect("every seat has its entity");
            world.entity_mut(player).insert(HeldOut);
            if defeat == Defeat::KillAll {
                doomed.clear();
                let of = units
                    .iter(world)
                    .filter(|(.., owner, _)| owner.seat() == seat);
                doomed.extend(of.map(|(&id, ..)| id));
                for &unit in &*doomed {
                    DamagePass::kill(world, unit);
                }
            }
        }
        if world.match_end().is_some() {
            return;
        }
        let first = holding.first().map(|held| held.team);
        let result = match first {
            None => MatchResult::Draw,
            Some(team) if holding.iter().all(|held| held.team == team) => MatchResult::Won(team),
            Some(_) => return,
        };
        let tick = world.resource::<SimTick>().start();
        let entity = world
            .match_entity()
            .expect("a match that runs has its entity");
        world.entity_mut(entity).insert(MatchEnd::new(tick, result));
    }
}
