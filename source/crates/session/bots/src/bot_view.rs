use std::fmt;

use bevy_ecs::query::{Allow, Has};
use bevy_ecs::system::{Query, Res, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionKind, ActionProgress, ActionSlots, TargetTest, Targets};
use campfire_capabilities::BotFacts;
use campfire_common::{PlayerSlot, Tick};
use campfire_interaction::Using;
use campfire_items::{Inventory, Shopping};
use campfire_players::OnMatch;
use campfire_production::{BuildPlaces, Builder, GatherView, Gatherer, PlaceSearch, StartChecks};
use campfire_script::Filter;
use campfire_sim::{EntityIndex, Position, StableId, Unpredicted};
use campfire_units::{Commanded, Dead, Destination, GroundPoint, MapBook, PlayerUnits};
use campfire_values::{Block, ItemId, Relations, TagSet, Team, UnitTags, UnitType};
use campfire_vision::{ByHolding, ByVision, SeenTargets, TeamSight};

use crate::loaded::ActionName;

/// What a bot's due order reads of the match, as a client holds it or a server, by `S`: its
/// player's units, each unit's point, team, slots and tags, the units its team sees as an order
/// may aim at them, the teams' relations, the map's metric, the places a build passes, and the
/// trains and builds each unit may start. A client may hold no unit's life,
/// as a mode's visibility may keep it from all but its owner, so a living unit is one with no
/// `Dead`; an attack's target is one the attack order's own test takes, which knows a life the
/// client does not hold by the unit's type.
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
    inventories: Query<'w, 's, &'static Inventory, Allow<Unpredicted>>,
    book: Res<'w, ActionBook>,
    targets: Targets<'w, 's>,
    gathers: Option<GatherView<'w, 's>>,
    using: Option<Using<'w, 's>>,
    shopping: Option<Shopping<'w, 's>>,
}

/// What a unit an order names is to the order, by which a search for it tests it as the order's
/// own test does (04-capabilities/control.md, A bot's search runs its order's own test).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitRole {
    /// Any unit the order's team sees, as a rally takes one.
    Seen,
    Attacked,
    /// The target of the action in the order's slot: a cast's, or a gather's node.
    Aimed(u8),
    Used,
    Shop,
    /// The site the build in the order's slot goes on with.
    Site(u8),
}

/// A bot's view of a client's world: the units its client holds are those its team sees.
pub type ClientBotView<'w, 's> = BotView<'w, 's, ByHolding>;

/// A bot's view of a server's world: the units its team sees, by the match's vision.
pub type ServerBotView<'w, 's> = BotView<'w, 's, ByVision>;

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

/// A `BotView` of a server's world, held for a caller with the world and no system of its own.
pub struct HeldBotView(SystemState<ServerBotView<'static, 'static>>);

impl<S: TeamSight> BotView<'_, '_, S> {
    /// The avatar of the player of `player_slot`, when it has one.
    pub fn avatar(&self, player_slot: PlayerSlot) -> Option<Commanded> {
        self.players.avatar(player_slot)
    }

    /// Puts in `out` the stable ids of the living units of `unit_type` the player of `player_slot`
    /// owns, in increasing order.
    pub(crate) fn typed(
        &self,
        player_slot: PlayerSlot,
        unit_type: UnitType,
        out: &mut Vec<StableId>,
    ) {
        self.players.typed(player_slot, unit_type, out);
    }

    /// Puts in `out` the stable ids of the living units of `ids`, rising, the player of
    /// `player_slot` owns, in increasing order.
    pub(crate) fn listed(
        &self,
        player_slot: PlayerSlot,
        ids: &[StableId],
        out: &mut Vec<StableId>,
    ) {
        self.players.listed(player_slot, ids, out);
    }

    /// Puts in `facts` what the units of the player of `player_slot` are in tick `now`: each train
    /// and build each may start, as the order's own checks find them, by unit, rising, then by
    /// slot; and each that stands idle, with no action under way, no target, no destination, no
    /// gather loop and no build order (04-capabilities/control.md, A bot gathers).
    pub(crate) fn facts(
        &self,
        player_slot: PlayerSlot,
        now: Tick,
        owned: &mut Vec<StableId>,
        facts: BotFacts<'_>,
    ) {
        let BotFacts {
            startable: starts,
            idle,
        } = facts;
        self.players.owned(player_slot, owned);
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
        debug_assert!(
            starts.is_sorted_by_key(|&(id, _)| id) && idle.is_sorted(),
            "the units of a bot's facts rise"
        );
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

    /// The place of the first stack of `item` that the unit `unit` carries, when it carries one.
    pub(crate) fn carried(&self, unit: StableId, item: ItemId) -> Option<u8> {
        let inventory = self.inventories.get(self.index.get(unit)?).ok()?;
        let place = (inventory.slots().iter())
            .position(|held| held.is_some_and(|held| held.item == item))?;
        Some(u8::try_from(place).expect("an inventory's places fit a slot's"))
    }

    /// The nearest to the first of `units` of the living units `filter` selects for its team that
    /// its team sees, as an order may aim only at those, whose tags let them be targets, and that
    /// the order of one of `units` takes as `role`, as Zero Hour's AI searches by its commands'
    /// own tests and OpenRA's bot by its attacks' valid weapons: the least distance between their
    /// points in the map's metric, exactly, and of those the least stable id, so the query's order
    /// chooses nothing.
    pub(crate) fn nearest(
        &self,
        units: &[StableId],
        role: UnitRole,
        filter: Filter,
    ) -> Option<StableId> {
        self.nearest_where(units[0], filter, |target| {
            units.iter().any(|&unit| self.takes(unit, role, target))
        })
    }

    /// Whether one of the weapons of the unit `unit` selects `target`, as its attack order's test
    /// does.
    pub(crate) fn may_attack(&self, unit: StableId, target: StableId) -> bool {
        self.takes(unit, UnitRole::Attacked, target)
    }

    /// Where on the ground the unit `id` stands; none for a unit the view does not hold.
    pub(crate) fn ground_of(&self, id: StableId) -> Option<GroundPoint> {
        let (_, position, ..) = self.units.get(self.index.get(id)?).ok()?;
        let at = position.get();
        Some(GroundPoint { x: at.x, z: at.z })
    }

    /// Whether an order of the unit `unit` takes `target` as `role`, by the test of the order's
    /// capability; none of a capability the match does not run.
    fn takes(&self, unit: StableId, role: UnitRole, target: StableId) -> bool {
        let Some(entity) = self.index.get(unit) else {
            return false;
        };
        let Ok((_, _, &team, slots, tags, _)) = self.units.get(entity) else {
            return false;
        };
        let test = TargetTest {
            book: &self.book,
            targets: &self.targets,
            team,
            tags: tags.map_or(TagSet::NONE, |tags| tags.tags),
        };
        match role {
            UnitRole::Seen => true,
            UnitRole::Attacked => slots.is_some_and(|slots| slots.may_attack(&test, target)),
            UnitRole::Aimed(slot) => {
                slots.is_some_and(|slots| match slots.kind_in(&self.book, slot) {
                    Some(ActionKind::Gather) => {
                        slots.usable(&self.book, slot, test.tags)
                            && (self.gathers.as_ref())
                                .is_some_and(|gathers| gathers.gathers(unit, slot, target))
                    }
                    _ => slots.may_aim(&test, slot, target),
                })
            }
            UnitRole::Used => {
                (self.using.as_ref()).is_some_and(|using| using.of(entity, target).is_some())
            }
            UnitRole::Shop => (self.shopping.as_ref())
                .is_some_and(|shopping| shopping.serving(target, unit).is_some()),
            UnitRole::Site(slot) => self.places.takes_site(unit, slot, target),
        }
    }

    /// The nearest to the unit `from` of the units `nearest` finds that `takes` takes.
    fn nearest_where(
        &self,
        from: StableId,
        filter: Filter,
        takes: impl Fn(StableId) -> bool,
    ) -> Option<StableId> {
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
                    && takes(id)
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

    pub fn get<'w, 's>(&'s mut self, world: &'w World) -> ServerBotView<'w, 's> {
        self.0.get(world).expect("its params are always valid")
    }
}

/// Its state holds the world's ids of what it reads, which print nothing of use.
impl fmt::Debug for HeldBotView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HeldBotView")
    }
}
