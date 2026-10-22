use std::fmt;

use bevy_ecs::query::{Allow, Has};
use bevy_ecs::system::{Query, Res, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots};
use campfire_capabilities::BotFacts;
use campfire_common::{Seat, Tick};
use campfire_players::OnMatch;
use campfire_production::{BuildPlaces, Builder, Gatherer, PlaceSearch, StartChecks};
use campfire_script::Filter;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Commanded, Dead, Destination, GroundPoint, MapBook, PlayerUnits};
use campfire_values::{ActionId, Block, Relations, Team, UnitTags, UnitType};
use campfire_vision::{ByVision, SeenTargets, TeamSight};

use crate::loaded::ActionName;

/// What a bot's due order reads of the match, as a client holds it or a server, by `S`: its
/// player's units, each unit's point, team, slots and tags, the units its team sees as an order
/// may aim at them, the teams' relations, the map's metric, the places a build passes, and the
/// trains and builds each unit may start. A client may hold no unit's life,
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
    places: BuildPlaces<'w, 's, S>,
    starts: StartChecks<'w, 's>,
    orders: Query<'w, 's, UnitOrders, Allow<Unpredicted>>,
}

/// What a `BotView` reads of a unit's orders: the action under way and its target, where it walks
/// to, its gather loop and its build order.
type UnitOrders = (
    Option<&'static ActionProgress>,
    Option<&'static Destination>,
    Option<&'static Gatherer>,
    Option<&'static Builder>,
);

/// What a `BotView` reads of each unit.
type UnitRow = (
    &'static StableId,
    &'static Position,
    &'static Team,
    Option<&'static ActionSlots>,
    Option<&'static UnitTags>,
    Has<Dead>,
);

/// What a bot's think reads of its units that the world's systems tell, as found before the
/// think: each unit, by stable id, with the action of each slot it may start, and the units that
/// stand idle, each rising; and the units the seat owns, as it lists them.
#[derive(Debug, Default)]
pub(crate) struct ThinkFacts {
    pub(crate) starts: Vec<(StableId, ActionId)>,
    pub(crate) idle: Vec<StableId>,
    pub(crate) owned: Vec<StableId>,
}

impl ThinkFacts {
    /// Them as a bot's call reads them.
    pub(crate) fn of_call(&self) -> BotFacts<'_> {
        BotFacts {
            startable: &self.starts,
            idle: &self.idle,
        }
    }
}

/// A `BotView` of a server's world, held for a caller with the world and no system of its own.
pub struct HeldBotView(SystemState<BotView<'static, 'static, ByVision>>);

impl<S: TeamSight> BotView<'_, '_, S> {
    /// The avatar of the player of `seat`, when it has one.
    pub fn avatar(&self, seat: Seat) -> Option<Commanded> {
        self.players.avatar(seat)
    }

    /// Puts in `out` the stable ids of the living units of `unit_type` the player of `seat` owns,
    /// in increasing order.
    pub(crate) fn typed(&self, seat: Seat, unit_type: UnitType, out: &mut Vec<StableId>) {
        self.players.typed(seat, unit_type, out);
    }

    /// Puts in `out` the stable ids of the living units of `ids`, rising, the player of `seat`
    /// owns, in increasing order.
    pub(crate) fn listed(&self, seat: Seat, ids: &[StableId], out: &mut Vec<StableId>) {
        self.players.listed(seat, ids, out);
    }

    /// Puts in `facts` what the units of the player of `seat` are in tick `now`: each train and
    /// build each may start, as the order's own checks find them, by unit, rising, then by slot;
    /// and each that stands idle, with no action under way, no target, no destination, no gather
    /// loop and no build order (04-capabilities/control.md, A bot gathers).
    pub(crate) fn facts(&self, seat: Seat, now: Tick, facts: &mut ThinkFacts) {
        let ThinkFacts {
            starts,
            idle,
            owned,
        } = facts;
        self.players.owned(seat, owned);
        starts.clear();
        idle.clear();
        idle.extend(owned.iter().copied().filter(|&id| self.idle(id)));
        for &id in &*owned {
            let Some((.., Some(slots), _, _)) = self
                .index
                .get(id)
                .and_then(|entity| self.units.get(entity).ok())
            else {
                continue;
            };
            let held = slots
                .indexed()
                .filter_map(|(at, slot)| Some((at, slot.action?)));
            for (at, action) in held {
                if self.starts.may_start(id, at, now) {
                    starts.push((id, action));
                }
            }
        }
    }

    /// Whether the unit `id` stands idle: none of its parts holds an order under way.
    fn idle(&self, id: StableId) -> bool {
        let Some(entity) = self.index.get(id) else {
            return false;
        };
        let Ok((progress, destination, gatherer, builder)) = self.orders.get(entity) else {
            return false;
        };
        let acts = progress.is_some_and(|progress| {
            progress.in_progress().is_some() || progress.attack_target().is_some()
        });
        let walks = destination.is_some_and(|destination| destination.get().is_some());
        let gathers = gatherer.is_some_and(|gatherer| gatherer.order().is_some());
        let builds = builder.is_some_and(|builder| builder.order().is_some());
        !(acts || walks || gathers || builds)
    }

    /// The nearest place of `search` where its build passes, as its order tests it.
    pub(crate) fn place(&self, search: PlaceSearch) -> Option<GroundPoint> {
        self.places.nearest(search)
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
        let (_, position, &team, ..) = self.units.get(self.index.get(from)?).ok()?;
        let (from, metric) = (position.get(), self.map.metric());
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
        let nearest = targets.min_by_key(|&(&id, position, ..)| {
            (
                metric.offset(from, position.get()).length_squared_bits(),
                id,
            )
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
