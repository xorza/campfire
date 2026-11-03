use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Added, Allow, Changed, Has, Or, QueryState, With, Without};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots, ActionTarget, InProgress, ProgressIndexes};
use campfire_common::PlayerSlot;
use campfire_declarations::ById;
use campfire_math::{Num, SinCos, Vec3};
use campfire_sim::{
    DueIndex, DueUnit, EntityIndex, IdAllocator, PassesResumed, Position, SimTick, StableId,
    UnitDues,
};
use campfire_stats::{Held, HeldModifiers};
use campfire_units::{
    Body, Dead, Facing, FiringRider, FlightGoal, Leaving, Life, MapBook, OpenCellAsk, OpenCellRun,
    OrderKind, Owner, RiderFate, RidersCalledOut, SeeingRider, SpawnAt, StatusTags, TagBook,
    UnitOrdered, Walk,
};
use campfire_values::{Rank, TagSet, Team, UnitTags, UnitType};

use crate::Interaction;
use crate::enter_spec::EnterSpec;
use crate::rider_size::RiderSize;
use crate::riders::Riders;
use crate::riding::{RiderParts, Riding, SpawnsInside};

/// The containers' passes: the exits their orders and their falls ask for, the riders they carry,
/// and the riders they spawn with (04-capabilities/interaction.md, A rider leaves at a free cell by
/// the container; 04-capabilities/interaction.md, A container's fall hurts its riders).
#[derive(Debug)]
pub(crate) struct Containers;

/// The containers that died this tick, with their stable ids.
type Fallen = QueryState<(Entity, &'static StableId), (With<Riders>, Added<Dead>)>;

/// The living containers whose life changed this tick, with their stable ids, lives and types.
type Hurt = QueryState<
    (Entity, &'static StableId, &'static Life, &'static UnitType),
    (With<Riders>, Changed<Life>, Without<Dead>),
>;

/// The containers that moved or turned this tick, or whose riders changed.
type Moved<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        Option<&'static Facing>,
        &'static Riders,
        &'static UnitType,
    ),
    Or<(Changed<Position>, Changed<Facing>, Changed<Riders>)>,
>;

/// The firing riders whose targets changed this tick.
type Retargeted<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static Riding,
        &'static ActionProgress,
        &'static mut Position,
    ),
    (With<FiringRider>, Changed<ActionProgress>, Allow<Disabled>),
>;

/// The containers, with their places, their facings and their riders.
type Holders<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        Option<&'static Facing>,
        &'static mut Riders,
        &'static UnitType,
    ),
    Without<Riding>,
>;

/// The living units in the world that may come to a container, with their actions.
type Boarders<'w, 's> = Query<
    'w,
    's,
    (&'static mut ActionProgress, &'static ActionSlots),
    (Without<Dead>, Without<Riding>),
>;

/// The living containers whose tags changed, with their riders and their types.
type Retagged<'w, 's> = Query<
    'w,
    's,
    (&'static Riders, &'static UnitType, &'static UnitTags),
    (Changed<UnitTags>, Without<Dead>),
>;

/// The containers that fly, with their places and their flight goals.
type Landers<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static mut Riders,
        &'static Position,
        &'static mut FlightGoal,
        &'static UnitType,
    ),
    Without<Dead>,
>;

/// A container's place and facing, which its fire points turn with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContainerFrame {
    at: Vec3,
    turn: SinCos,
}

impl ContainerFrame {
    /// The frame of the container of `entity`.
    pub(crate) fn of_unit(world: &World, entity: Entity) -> Self {
        let at = world
            .get::<Position>(entity)
            .expect("a container stands")
            .get();
        Self::of(at, world.get::<Facing>(entity))
    }

    fn of(at: Vec3, facing: Option<&Facing>) -> Self {
        let angle = facing.map_or(Num::ZERO, |facing| facing.degrees());
        Self {
            at,
            turn: SinCos::of_degrees(angle),
        }
    }

    /// Where a rider of the fire point `point` of `points` stands, or with none the container.
    pub(crate) fn rider_at(self, points: &[[Num; 2]], point: Option<u8>) -> Vec3 {
        point.map_or(self.at, |at| self.place(points[usize::from(at)]))
    }

    /// The point `[x, z]` of the frame, turned with it from `x` towards `−z`, at its height.
    pub(crate) const fn place(self, [x, z]: [Num; 2]) -> Vec3 {
        let turned = Vec3::new(x, Num::ZERO, z).rotated_y(self.turn);
        self.at
            .checked_add(turned)
            .expect("a fire point within the bound of a place")
    }
}

/// The riders' places, out of the world.
type RiderPlaces<'w, 's> = Query<'w, 's, &'static mut Position, (With<Riding>, Allow<Disabled>)>;

/// A container and where it stands, for the riders it carries. Scratch.
#[derive(Debug, Default)]
pub(crate) struct CarriedRiders(Vec<(Vec3, StableId)>);

/// What an assault reads: the containers' specs, the riders' lives, out of the world as they ride,
/// and the index of the units.
type AssaultReads<'w, 's> = (
    Res<'w, ById<UnitType, EnterSpec>>,
    Query<'w, 's, &'static Life, Allow<Disabled>>,
    Res<'w, EntityIndex>,
);

impl Containers {
    /// The observer of an exit, an evacuation or an attack ordered to a container, on the server:
    /// its rider, or each, or each of an assault transport whose life lies above its share, leaves
    /// after the riders that leave before it, as its exits come due; an exit of a unit that does
    /// not ride in it does nothing (04-capabilities/interaction.md, A transport may assault).
    pub(crate) fn ordered(
        ordered: On<'_, '_, UnitOrdered>,
        mut containers: Query<'_, '_, (&StableId, &UnitType, &mut Riders), Without<Dead>>,
        (specs, lives, units): AssaultReads<'_, '_>,
        index: Option<ResMut<'_, DueIndex<Riders>>>,
    ) {
        // The exits are the server's, whose riders come back by replication.
        let (Some(mut index), Ok((&id, &unit_type, mut riders))) =
            (index, containers.get_mut(ordered.entity))
        else {
            return;
        };
        match ordered.order {
            OrderKind::Exit { rider } => {
                if !riders.let_out(rider) {
                    return;
                }
            }
            OrderKind::Evacuate if !riders.is_empty() => riders.evacuate(),
            OrderKind::Attack => {
                let Some(share) = specs.get(unit_type).and_then(|spec| spec.assault) else {
                    return;
                };
                let fit = |rider: StableId| {
                    let life = units.get(rider).and_then(|rider| lives.get(rider).ok());
                    life.is_some_and(|life| !Riders::at_most(life, share))
                };
                let leaving: Vec<StableId> = riders
                    .iter()
                    .map(|rider| rider.id)
                    .filter(|&rider| fit(rider))
                    .collect();
                let mut any = false;
                for rider in leaving {
                    any |= riders.let_out(rider);
                }
                if !any {
                    return;
                }
            }
            _ => return,
        }
        UnitDues::new(&mut index, id, ordered.entity).push(riders.exit_due());
    }

    /// Lets out each rider whose exit came, of the containers the index holds due, by their stable
    /// ids, each its container's exit time after the one before, at the free cell nearest its
    /// container (04-capabilities/interaction.md, A rider leaves at a free cell by the container).
    pub(crate) fn let_out(world: &mut World, mut taken: Local<'_, Vec<DueUnit>>) {
        let now = world.resource::<SimTick>().start();
        taken.clear();
        world
            .resource_mut::<DueIndex<Riders>>()
            .take_due(now, &mut taken);
        for &DueUnit { id, entity } in &*taken {
            let Some(exit) = Self::spec(world, entity).map(|spec| spec.exit) else {
                continue;
            };
            if world.entity(entity).contains::<Dead>() {
                continue;
            }
            // A container in the air lets no rider out: it lands, and they leave on the ground
            // (04-capabilities/navigation.md, A flier lands for its riders).
            if !Riders::grounded(world, entity) {
                UnitDues::<Riders>::with(world, id, entity, |world, dues| {
                    let riders = world
                        .get::<Riders>(entity)
                        .expect("a container holds riders");
                    dues.push(riders.exit_due());
                });
                continue;
            }
            while let Some(rider) = world
                .get_mut::<Riders>(entity)
                .and_then(|mut riders| riders.leave(now, exit))
            {
                let inside = world
                    .get::<Riders>(entity)
                    .expect("a container")
                    .iter()
                    .len();
                Self::put_out(world, entity, LeftRider { id: rider, inside });
            }
            Self::emptied(world, entity);
            UnitDues::<Riders>::with(world, id, entity, |world, dues| {
                let riders = world
                    .get::<Riders>(entity)
                    .expect("a container holds riders");
                dues.push(riders.exit_due());
            });
        }
    }

    /// Lets every rider out of each container that died this tick, each dealt its container's
    /// `damage` share of its life's most, which dies at all of it, as Zero Hour's
    /// `OpenContain::onDie`; and starts the exit of every rider of each living container whose
    /// life fell to its `full` share, as `GarrisonContain::onBodyDamageStateChange`
    /// (04-capabilities/interaction.md, A container's fall hurts its riders).
    pub(crate) fn fall(
        world: &mut World,
        dead: &mut Fallen,
        hurt: &mut Hurt,
        mut fallen: Local<'_, Vec<(StableId, Entity)>>,
    ) {
        fallen.clear();
        fallen.extend(dead.iter(world).map(|(entity, &id)| (id, entity)));
        fallen.sort_unstable();
        for &(_, container) in &*fallen {
            let Some(damage) = Self::spec(world, container).map(|spec| spec.damage) else {
                continue;
            };
            let riders = world
                .get_mut::<Riders>(container)
                .expect("a container holds riders")
                .empty();
            let count = riders.len();
            for (at, rider) in riders.into_iter().enumerate() {
                let left = LeftRider {
                    id: rider.id,
                    inside: count - 1 - at,
                };
                let Some(entity) = Self::put_out(world, container, left) else {
                    continue;
                };
                if let Some(mut life) = world.get_mut::<Life>(entity) {
                    let dealt = damage.of_num(life.max());
                    life.take(dealt);
                }
            }
            Self::emptied(world, container);
        }
        fallen.clear();
        let specs = world.resource::<ById<UnitType, EnterSpec>>();
        fallen.extend(
            hurt.iter(world)
                .filter(|&(_, _, life, &unit_type)| {
                    (specs.get(unit_type)).is_some_and(|spec| Riders::at_most(life, spec.full))
                })
                .map(|(entity, &id, ..)| (id, entity)),
        );
        for &(id, container) in &*fallen {
            UnitDues::<Riders>::with(world, id, container, |world, dues| {
                let mut riders = world.get_mut::<Riders>(container).expect("a container");
                if !riders.is_empty() {
                    riders.evacuate();
                    dues.push(riders.exit_due());
                }
            });
        }
    }

    /// Puts each rider where its container stands, or at its fire point of its container's frame,
    /// of the containers that moved or turned this tick or whose riders changed, as a rider's place
    /// is its container's while it rides (04-capabilities/combat.md, A container may let its riders
    /// fire).
    pub(crate) fn carry(
        mut units: ParamSet<'_, '_, (Moved<'_, '_>, RiderPlaces<'_, '_>)>,
        (index, specs): (Res<'_, EntityIndex>, Res<'_, ById<UnitType, EnterSpec>>),
        mut carried: Local<'_, CarriedRiders>,
    ) {
        carried.0.clear();
        for (position, facing, riders, &unit_type) in &units.p0() {
            let points = specs
                .get(unit_type)
                .map_or(&[][..], |spec| spec.fire.points());
            let frame = ContainerFrame::of(position.get(), facing);
            let stands = position.get();
            carried.0.extend(riders.iter().map(|rider| {
                let at = rider
                    .point
                    .map_or(stands, |at| frame.place(points[usize::from(at)]));
                (at, rider.id)
            }));
        }
        let mut riders = units.p1();
        for &(at, rider) in &carried.0 {
            if let Some(mut position) = index
                .get(rider)
                .and_then(|rider| riders.get_mut(rider).ok())
            {
                position.set_if_neq(Position::from(at));
            }
        }
    }

    /// Moves each firing rider of a container of fire points whose target changed to the free
    /// point nearest its target, when it is nearer than its own, there at once
    /// (04-capabilities/combat.md, A container may let its riders fire).
    pub(crate) fn aim(
        mut firing: Retargeted<'_, '_>,
        mut containers: Holders<'_, '_>,
        targets: Query<'_, '_, &Position, (Without<Riding>, Without<Riders>)>,
        (index, specs): (Res<'_, EntityIndex>, Res<'_, ById<UnitType, EnterSpec>>),
        mut placed: Local<'_, Vec<Vec3>>,
    ) {
        for (&id, riding, progress, mut position) in &mut firing {
            let Some(target) = progress.aimed_at().and_then(|target| index.get(target)) else {
                continue;
            };
            let Some(container) = index.get(riding.container()) else {
                continue;
            };
            let Ok((at, facing, mut riders, &unit_type)) = containers.get_mut(container) else {
                continue;
            };
            let points = specs
                .get(unit_type)
                .map_or(&[][..], |spec| spec.fire.points());
            let (Ok(target), false) = (targets.get(target), points.is_empty()) else {
                continue;
            };
            let frame = ContainerFrame::of(at.get(), facing);
            placed.clear();
            placed.extend(points.iter().map(|&point| frame.place(point)));
            // The part is marked changed only as a point moves, so a retarget that keeps it is
            // not sent and hashed again, nor its riders placed again.
            if riders
                .bypass_change_detection()
                .aim(id, &placed, target.get())
            {
                riders.set_changed();
                let point = riders
                    .iter()
                    .find(|rider| rider.id == id)
                    .and_then(|rider| rider.point);
                let stands = at.get();
                let to = point.map_or(stands, |point| placed[usize::from(point)]);
                position.set_if_neq(Position::from(to));
            }
        }
    }

    /// Lands each container that flies while a unit comes to it or waits for it, or a rider waits
    /// to leave it, once it walks nowhere, where its body has room on the ground, and lifts it
    /// again once none does; once it is on the ground, each unit that waits for it enters again,
    /// and a unit that took another order or went comes no more (04-capabilities/navigation.md, A
    /// flier lands for its riders; riders-and-upgrades.md, A flier lands where Zero Hour's
    /// transport lands).
    pub(crate) fn land(
        mut fliers: Landers<'_, '_>,
        mut boarders: Boarders<'_, '_>,
        (index, specs): (Res<'_, EntityIndex>, Res<'_, ById<UnitType, EnterSpec>>),
        mut indexes: ProgressIndexes<'_>,
    ) {
        for (&id, mut riders, position, mut goal, &unit_type) in &mut fliers {
            let Some(spec) = specs.get(unit_type) else {
                continue;
            };
            let grounded = Riders::stands(position.get(), true);
            if !riders.wants_ground() && !riders.lands() {
                continue;
            }
            // The part is marked changed only as a boarder comes, goes or stops waiting, so a
            // container whose boarders walk to it is not sent and hashed again each tick.
            let mut changed = false;
            riders
                .bypass_change_detection()
                .boarders_mut()
                .retain_mut(|boarder| {
                    let Some((entity, (mut progress, slots))) = index
                        .get(boarder.id)
                        .and_then(|entity| Some((entity, boarders.get_mut(entity).ok()?)))
                    else {
                        changed = true;
                        return false;
                    };
                    let entry = slots
                        .indexed()
                        .find(|(_, slot)| slot.action == Some(spec.action))
                        .map(|(at, _)| at);
                    let coming = progress.in_progress().is_some_and(|underway| {
                        matches!(underway, InProgress::Order { aim, .. }
                        if aim.target == ActionTarget::Unit(id) && Some(aim.slot) == entry)
                    });
                    if !boarder.waits {
                        changed |= !coming;
                        return coming;
                    }
                    // A unit that waits and took another order comes no more.
                    if progress.in_progress().is_some() && !coming {
                        changed = true;
                        return false;
                    }
                    if grounded && let Some(slot) = entry {
                        progress.order(
                            slot,
                            ActionTarget::Unit(id),
                            &mut indexes.of(boarder.id, entity),
                        );
                        boarder.waits = false;
                        changed = true;
                    }
                    true
                });
            if changed {
                riders.set_changed();
            }
            let lands = riders.wants_ground();
            if lands {
                if !goal.is_landing() {
                    *goal = FlightGoal::Land;
                }
            } else if riders.lands() {
                goal.set_if_neq(FlightGoal::Height);
            }
            if riders.lands() != lands {
                riders.set_lands(lands);
            }
        }
    }

    /// Holds each container's modifier on each of its riders this tick, from the container, in a
    /// match with stats (04-capabilities/combat.md, A container's modifier holds on its riders).
    pub(crate) fn hold(
        containers: Query<'_, '_, (&StableId, &Riders, &UnitType), Without<Dead>>,
        specs: Res<'_, ById<UnitType, EnterSpec>>,
        held: Option<ResMut<'_, HeldModifiers>>,
    ) {
        let Some(mut held) = held else {
            return;
        };
        for (&id, riders, &unit_type) in &containers {
            let Some(modifier) = specs.get(unit_type).and_then(|spec| spec.modifier) else {
                continue;
            };
            for rider in riders.iter() {
                held.hold(Held {
                    target: rider.id,
                    modifier,
                    source: Some(id),
                    action: None,
                    rank: Rank::FIRST,
                });
            }
        }
    }

    /// Spawns the riders of the payload of the container of `entity`, which the spawner spawned,
    /// inside it, in their order, each of its team and its player, as it comes full
    /// (04-capabilities/interaction.md, A container may come full).
    pub(crate) fn fill(world: &mut World, entity: Entity) {
        let Some(spec) = Self::spec(world, entity).filter(|spec| !spec.payload.is_empty()) else {
            return;
        };
        let spec = spec.clone();
        let container = world.entity(entity);
        let container_id = *container.get::<StableId>().expect("a container has an id");
        let position = container
            .get::<Position>()
            .expect("a container stands")
            .get();
        let team = *container.get::<Team>().expect("a container has a team");
        let owner: Option<PlayerSlot> = container.get::<Owner>().map(|owner| owner.player_slot());
        let held = container
            .get::<UnitTags>()
            .map_or(TagSet::NONE, |held| held.tags);
        let points = spec.fire.points().len();
        for each in &spec.payload {
            let size = world
                .resource::<ById<UnitType, RiderSize>>()
                .get(each.unit_type);
            let room = spec.counts.room_of(size.map_or(1, |size| size.get()));
            let tags = world.resource::<TagBook>().own(each.unit_type);
            let parts = RiderParts {
                riding: Riding::new(container_id),
                team,
                fires: spec.rider_fires(tags, held),
                sees: spec.sight,
            };
            for _ in 0..each.count {
                let at = SpawnAt {
                    id: world.resource_mut::<IdAllocator>().allocate(),
                    unit_type: each.unit_type,
                    team,
                    owner,
                    position,
                    angle: Num::ZERO,
                    variation: None,
                };
                parts.give(SpawnsInside(world, at));
                let mut riders = world.get_mut::<Riders>(entity).expect("a container");
                riders.take(at.id, room, points);
            }
        }
        Self::holding(world, entity, true);
    }

    /// Puts `left`, a rider that left the container of `entity`, out at the center of the free
    /// cell nearest the container that its walker may stand in, else where the container stands,
    /// leaving through its team's units, and walking out of the container's body to its place of
    /// the container's `ExitBlock`, where it comes out in the body; its entity, of a rider the
    /// match holds.
    fn put_out(world: &mut World, entity: Entity, left: LeftRider) -> Option<Entity> {
        let LeftRider { id: rider, inside } = left;
        let at = world
            .get::<Position>(entity)
            .expect("a container stands")
            .get();
        let unit = world.resource::<EntityIndex>().get(rider)?;
        let unit_type = *world.get::<UnitType>(unit).expect("a rider has a type");
        let position = Self::open_cell(world, unit_type, at).unwrap_or(at);
        let bound = world.get::<Body>(unit).map(|body| body.shape().bound());
        let way_out = bound
            .zip(ExitBlock::of(world, entity))
            .and_then(|(bound, block)| {
                let place = block.place(position, bound, inside)?;
                Some(
                    world
                        .resource::<MapBook>()
                        .bounds
                        .ground_point(place, block.stands),
                )
            });
        let mut out = world.entity_mut(unit);
        out.remove::<(Riding, FiringRider, SeeingRider)>();
        *out.get_mut::<Position>().expect("a rider stands") = Position::from(position);
        // The deaths read no rider, so one whose life a drain took while it rode dies as it comes
        // out (04-capabilities/interaction.md, A rider dies as it comes out).
        if let Some(mut life) = out.get_mut::<Life>() {
            life.set_changed();
        }
        if let Some(mut leaving) = out.get_mut::<Leaving>() {
            Leaving::set(&mut leaving, true);
        }
        if let Some(to) = way_out {
            Walk::go_to(world, unit, Some(to));
        }
        Some(unit)
    }

    /// The center of the cell nearest `near` that a walker of `unit_type` may stand in, as a
    /// teleport finds its cell; none in a match of no such search, or with no such cell.
    fn open_cell(world: &mut World, unit_type: UnitType, near: Vec3) -> Option<Vec3> {
        let &OpenCellRun(run) = world.get_resource::<OpenCellRun>()?;
        let ask = OpenCellAsk { unit_type, near };
        world.run_system_with(run, ask).expect("the search runs")
    }

    /// Takes the `holding` tag from the container of `entity` once its riders left, and gives a
    /// free building back to the player and the team it had (04-capabilities/interaction.md, A free
    /// building takes its riders' side).
    fn emptied(world: &mut World, entity: Entity) {
        let mut riders = world.get_mut::<Riders>(entity).expect("a container");
        if !riders.is_empty() {
            return;
        }
        let from = riders.give_back();
        Self::holding(world, entity, false);
        let Some(from) = from else {
            return;
        };
        let mut container = world.entity_mut(entity);
        container.insert(from.team);
        match from.owner {
            Some(owner) => drop(container.insert(Owner::new(owner))),
            None => drop(container.remove::<Owner>()),
        }
    }

    /// The observer of a container's riders called out as it leaves the match, on the server, which
    /// keeps the exits' index: each leaves at its exit time, as a sale starts, or those still
    /// inside go with it, as it goes (04-capabilities/interaction.md, A sold container lets its
    /// riders out).
    pub(crate) fn called_out(
        called: On<'_, '_, RidersCalledOut>,
        mut containers: Query<'_, '_, (&StableId, &mut Riders)>,
        (index, dues): (Res<'_, EntityIndex>, Option<ResMut<'_, DueIndex<Riders>>>),
        mut commands: Commands<'_, '_>,
    ) {
        let RidersCalledOut { entity, fate } = *called;
        let Ok((&id, mut riders)) = containers.get_mut(entity) else {
            return;
        };
        match fate {
            RiderFate::Leave => {
                if let Some(mut dues) = dues.filter(|_| !riders.is_empty()) {
                    riders.evacuate();
                    UnitDues::new(&mut dues, id, entity).push(riders.exit_due());
                }
            }
            RiderFate::Go => {
                for rider in riders.empty() {
                    if let Some(gone) = index.get(rider.id) {
                        commands.entity(gone).despawn();
                    }
                }
            }
        }
    }

    /// Marks each rider of a container whose tags changed, and whose fire waits for tags, as one
    /// that fires while the container's tags pass them, and takes the mark while they do not, as
    /// `PassengersFireUpgrade` lets the Helix's riders fire once it holds its bunker
    /// (riders-and-upgrades.md, A rider's fire follows its container's tags).
    pub(crate) fn arm(
        containers: Retagged<'_, '_>,
        (specs, index): (Res<'_, ById<UnitType, EnterSpec>>, Res<'_, EntityIndex>),
        riders: Query<'_, '_, (Option<&UnitTags>, Has<FiringRider>), Allow<Disabled>>,
        mut commands: Commands<'_, '_>,
    ) {
        for (held, &unit_type, tags) in &containers {
            let Some(spec) = specs.get(unit_type).filter(|spec| spec.fire_when.is_some()) else {
                continue;
            };
            for rider in held.iter() {
                let Some((entity, (rider_tags, marked))) = index
                    .get(rider.id)
                    .and_then(|entity| Some((entity, riders.get(entity).ok()?)))
                else {
                    continue;
                };
                let own = rider_tags.map_or(TagSet::NONE, |tags| tags.tags);
                match (spec.rider_fires(own, tags.tags), marked) {
                    (true, false) => {
                        commands.entity(entity).insert(FiringRider);
                        commands.trigger(PassesResumed { entity });
                    }
                    (false, true) => {
                        commands.entity(entity).remove::<FiringRider>();
                    }
                    _ => {}
                }
            }
        }
    }

    /// Gives the container of `entity` the `holding` tag when `on`, else takes it.
    pub(crate) fn holding(world: &mut World, entity: Entity, on: bool) {
        if let Some(mut tags) = world.get_mut::<StatusTags>(entity) {
            let turned = tags.turned(Interaction::HOLDING, on);
            tags.set_if_neq(turned);
        }
    }

    /// The `enter` of the container of `entity`'s type.
    fn spec(world: &World, entity: Entity) -> Option<&EnterSpec> {
        let unit_type = world.get::<UnitType>(entity)?;
        world
            .resource::<ById<UnitType, EnterSpec>>()
            .get(*unit_type)
    }
}

/// A rider that left its container, by its id, and the count of riders still inside as it left.
#[derive(Debug, Clone, Copy)]
struct LeftRider {
    id: StableId,
    inside: usize,
}

/// The places behind a container that its riders walk out to from where they come out in its
/// body, as a vehicle's riders come out under it: as many columns wide as the square root of its
/// capacity, rounded up, the places a rider's width apart, the first row touching its bound, so
/// each rider walks to a place of its own, as Zero Hour's `OpenContain::exitObjectViaDoor` walks
/// each rider to its own free place by its container's `ExitEnd`
/// (04-capabilities/interaction.md, A rider walks out of its container's body).
#[derive(Debug, Clone, Copy)]
struct ExitBlock {
    body: Body,
    stands: Vec3,
    way: [Num; 2],
    columns: u32,
}

impl ExitBlock {
    /// The block of the container of `entity`, by its body, its place, its facing and its spec's
    /// capacity; none for a container of no body.
    fn of(world: &World, entity: Entity) -> Option<Self> {
        let container = world.entity(entity);
        let body = *container.get::<Body>()?;
        let stands = (container.get::<Position>())
            .expect("a container stands")
            .get();
        let facing = (container.get::<Facing>().copied()).unwrap_or(Facing::of(Num::ZERO));
        let capacity = Containers::spec(world, entity).map_or(1, |spec| spec.capacity.max(1));
        let root = capacity.isqrt();
        let columns = if root * root < capacity {
            root + 1
        } else {
            root
        };
        Some(Self {
            body,
            stands,
            way: facing.way(),
            columns,
        })
    }

    /// The place, `[x, z]`, of a rider of the bound `rider` that came out at `at` with `inside`
    /// riders still in its container: the place of that count, by rows from the nearest, each row
    /// from its middle out to either side by turns, so the first to leave walks farthest and none
    /// walks through one that stands; none for a rider clear of the container's body there.
    fn place(self, at: Vec3, rider: Num, inside: usize) -> Option<[Num; 2]> {
        let shape = self.body.shape();
        shape.push_out(self.stands, at, rider)?;
        let place = u32::try_from(inside).expect("riders within a container's capacity");
        let (row, column) = (place / self.columns, place % self.columns);
        let side = i64::from(column.div_ceil(2)) * if column % 2 == 1 { 1 } else { -1 };
        let width = rider * Num::int(2);
        let back = shape.bound() + rider + width * i64::from(row);
        let across = width * side;
        let [x, z] = self.way;
        Some([
            self.stands.x - x * back - z * across,
            self.stands.z - z * back + x * across,
        ])
    }
}
