use std::fmt;

use bevy_ecs::query::{Allow, Has};
use bevy_ecs::system::{Query, Res, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_actions::ActionSlots;
use campfire_common::PlayerSlot;
use campfire_players::OnMatch;
use campfire_script::Filter;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Commanded, Dead, MapBook, PlayerUnits};
use campfire_values::{Block, Relations, Team, UnitTags, UnitType};
use campfire_vision::{ByVision, SeenTargets, TeamSight};

use crate::loaded::ActionName;

/// What a bot's due order reads of the match, as a client holds it or a server, by `S`: its
/// player's units, each unit's point, team, slots and tags, the units its team sees as an order
/// may aim at them, the teams' relations and the map's metric. A client may hold no unit's life,
/// as a mode's visibility may keep it from all but its owner, so a living unit is one with no
/// `Dead`.
#[derive(SystemParam, Debug)]
pub struct BotView<'w, 's, S: TeamSight> {
    players: PlayerUnits<'w, 's>,
    index: Res<'w, EntityIndex>,
    units: Query<'w, 's, UnitRow, Allow<Unpredicted>>,
    seen: SeenTargets<'w, 's, S>,
    relations: OnMatch<'w, 's, Relations>,
    map: Res<'w, MapBook>,
}

/// What a `BotView` reads of each unit.
type UnitRow = (
    &'static StableId,
    &'static Position,
    &'static Team,
    Option<&'static ActionSlots>,
    Option<&'static UnitTags>,
    Has<Dead>,
);

/// A `BotView` of a server's world, held for a caller with the world and no system of its own.
pub struct HeldBotView(SystemState<BotView<'static, 'static, ByVision>>);

impl<S: TeamSight> BotView<'_, '_, S> {
    /// The avatar of the player of `slot`, when it has one.
    pub fn avatar(&self, slot: PlayerSlot) -> Option<Commanded> {
        self.players.avatar(slot)
    }

    /// Puts in `out` the stable ids of the living units of `unit_type` the player of `slot` owns,
    /// in increasing order.
    pub(crate) fn typed(&self, slot: PlayerSlot, unit_type: UnitType, out: &mut Vec<StableId>) {
        self.players.typed(slot, unit_type, out);
    }

    /// The first slot of the unit `id` that holds an action of `name`, when it holds one.
    pub(crate) fn slot_of(&self, id: StableId, name: &ActionName) -> Option<u8> {
        let (.., slots, _, _) = self.units.get(self.index.get(id)?).ok()?;
        let holds = |action| name.0.contains(&action);
        let found = slots?
            .indexed()
            .find(|(_, slot)| slot.action.is_some_and(holds));
        found.map(|(at, _)| at)
    }

    /// The nearest to the unit `from` of the living units `filter` selects for its team that its
    /// team sees, as an order may aim only at those, and whose tags let them be targets: the
    /// least distance between their points in the map's metric, exactly, and of those the least
    /// stable id, so the query's order chooses nothing.
    pub(crate) fn nearest(&self, from: StableId, filter: Filter) -> Option<StableId> {
        let (_, pos, &team, ..) = self.units.get(self.index.get(from)?).ok()?;
        let (from, metric) = (pos.get(), self.map.metric);
        let relations = self.relations.get();
        let targets = self
            .units
            .iter()
            .filter(|&(&id, _, &other, _, tags, dead)| {
                let tags = tags.copied().unwrap_or_default();
                !dead
                    && !tags.properties.blocks(Block::Target)
                    && filter.selects(relations.between(team, other), tags.tags)
                    && self.seen.sees(team, id)
            });
        let nearest = targets.min_by_key(|&(&id, pos, ..)| {
            (metric.offset(from, pos.get()).length_squared_bits(), id)
        });
        nearest.map(|(&id, ..)| id)
    }
}

impl HeldBotView {
    pub fn new(world: &mut World) -> Self {
        Self(SystemState::new(world))
    }

    pub fn get<'w, 's>(&'s mut self, world: &'w World) -> BotView<'w, 's, ByVision> {
        self.0.get(world).expect("its params are always valid")
    }
}

/// Its state holds the world's ids of what it reads, which print nothing of use.
impl fmt::Debug for HeldBotView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HeldBotView")
    }
}
