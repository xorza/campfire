use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_names::{MatchNames, UnknownName};
use campfire_players::{HeldOut, PlayersWorld};
use campfire_sim::{Capability, SimResource, SimTick, StableId};
use campfire_units::{Dead, Owner, UnitTypes, UnitsWorld};
use campfire_values::{Named, PackageIndex, Tag, Team, UnitTags};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::match_end::{MatchEnd, MatchResult};
use crate::mode_world::ModeWorld;
use crate::victory_data::{Defeat, VictoryData};

/// The mode's victory by the last unit of a tag, as its `[victory]` gives it; none for a mode of
/// no `[victory]`. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[component(immutable)]
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

/// The players that held a living unit of the tag that counts at the victory's last count, by
/// player slot, which a player falls from once it holds none.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct VictoryHolders(Vec<PlayerSlot>);

/// A player that holds a living unit of the tag, and its team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Holding {
    player_slot: PlayerSlot,
    team: Team,
}

impl VictoryBook {
    /// The victory of the mode's `[victory]`, its tag resolved; an error for a tag the match does
    /// not declare, and in a match without combat, whose deaths it follows.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let Some(data) = cx.mode_section::<VictoryData>() else {
            return Ok(Self(None));
        };
        cx.require(Capability::Combat, PackageIndex::MODE, || {
            BookPlace::Section(VictoryData::KEY)
        })?;
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
    /// Counts the players that hold a living unit of the tag as the match starts, the ones its
    /// first fall is of; none for a mode of no victory.
    pub(crate) fn start(world: &mut World) {
        let Some(Self { counts, .. }) = world.resource::<VictoryBook>().victory() else {
            return;
        };
        let mut units = world.query_filtered::<LivingUnit, Without<Dead>>();
        let mut holding = Vec::new();
        Self::count(world, &mut units, counts, &mut holding);
        let player_slots = holding.iter().map(|holds| holds.player_slot).collect();
        world.insert_resource(VictoryHolders(player_slots));
    }

    /// The players that hold a living unit of `counts`, each with its team, into `holding`, by
    /// player slot.
    fn count(
        world: &World,
        units: &mut QueryState<LivingUnit, Without<Dead>>,
        counts: Tag,
        holding: &mut Vec<Holding>,
    ) {
        holding.clear();
        for (_, tags, owner, &team) in units.iter(world) {
            if tags.tags.contains(counts) {
                holding.push(Holding {
                    player_slot: owner.player_slot(),
                    team,
                });
            }
        }
        holding.sort_unstable();
        holding.dedup();
    }

    /// Counts the players that hold a living unit of the tag that counts: a player that held one
    /// at the last count and holds none is out of play and, as its defeat says, its other units
    /// die as the next tick's deaths find them, whether its last one died, went, or changed hands
    /// or tags; then, of a match that runs, the one team whose players hold living units of the
    /// tag wins, and of none the match is a draw. The tick's falls are followed together, so each
    /// finds the same units left (04-capabilities/games.md, Victory by the last unit of a tag).
    pub(crate) fn follow(
        world: &mut World,
        mut fallen: Local<'_, Vec<PlayerSlot>>,
        mut holding: Local<'_, Vec<Holding>>,
        mut doomed: Local<'_, Vec<StableId>>,
        mut units: Local<'_, QueryState<LivingUnit, Without<Dead>>>,
    ) {
        let victory = world.resource::<VictoryBook>().victory();
        let Self { counts, defeat } = victory.expect("the pass installs with a victory");
        Self::count(world, &mut units, counts, &mut holding);
        let held = |player_slot| holding.iter().any(|holds| holds.player_slot == player_slot);
        let holders = world.resource::<VictoryHolders>();
        fallen.clear();
        fallen.extend(
            holders
                .0
                .iter()
                .copied()
                .filter(|&player_slot| !held(player_slot)),
        );
        let now = holding.iter().map(|holds| holds.player_slot);
        if !holders.0.iter().copied().eq(now.clone()) {
            let mut holders = world.resource_mut::<VictoryHolders>();
            holders.0.clear();
            holders.0.extend(now);
        }
        if fallen.is_empty() {
            return;
        }
        for &player_slot in &*fallen {
            let player = world
                .player_entity(player_slot)
                .expect("every player slot has its entity");
            world.entity_mut(player).insert(HeldOut);
            if defeat == Defeat::KillAll {
                doomed.clear();
                let of = units
                    .iter(world)
                    .filter(|(.., owner, _)| owner.player_slot() == player_slot);
                doomed.extend(of.map(|(&id, ..)| id));
                for &unit in &*doomed {
                    world.drain_life(unit);
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

/// A snapshot is untrusted, so player slots out of order or twice fail to decode.
impl<'de> Deserialize<'de> for VictoryHolders {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let player_slots = Vec::<PlayerSlot>::deserialize(deserializer)?;
        if !player_slots.is_sorted_by(|a, b| a < b) {
            return Err(D::Error::custom("player slots in order, each once"));
        }
        Ok(Self(player_slots))
    }
}

impl SimResource for VictoryHolders {
    const NAME: &'static str = "mode.victory_holders";

    // Each holder is a player of the session.
    fn check(&self, world: &World) -> bool {
        let names = world.resource::<MatchNames>();
        self.0
            .iter()
            .all(|&player_slot| names.has_player(player_slot))
    }
}
