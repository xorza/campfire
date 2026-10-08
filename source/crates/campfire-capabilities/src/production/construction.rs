use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Changed, QueryState, Without};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res, SystemParam, SystemState};
use bevy_ecs::world::{Mut, World};
use campfire_common::Ticks;
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, IdAllocator, Keyed, Ordered, Position, SimTick, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::{ActionSlots, SlotAim};
use crate::actions::action_target::ActionTarget;
use crate::actions::kind_spec::KindSpec;
use crate::actions::purse::{Payer, Purse};
use crate::actions::range::Range;
use crate::navigation::destination::Destination;
use crate::navigation::route::Route;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::production::build_specs::{BuildSpec, BuildSpecs, Style};
use crate::production::build_target::BuildTarget;
use crate::production::builder::{BuildOrder, Builder};
use crate::production::holdings::{Held, Holdings};
use crate::production::placement::Placement;
use crate::production::requirements::Requirements;
use crate::production::site::Site;
use crate::stats::life_pool::LifePool;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pools::Pools;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::engine_tag::EngineTag;
use crate::units::owner::Owner;
use crate::units::spawner::{SpawnAt, Spawner};
use crate::units::status_tags::StatusTags;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::values::attitude::Attitude;
use crate::values::metric::Metric;
use crate::values::shape::Shape;

/// The systems of construction: a build order checked as it applies, builds that start in order
/// of their builders' stable ids, and sites that grow and complete.
#[derive(Debug)]
pub(crate) struct Construction;

/// What a build order reads of the match: the books, the players' resources and modifiers, the
/// placement, the builders, the sites, and the living, complete units the players own.
#[derive(SystemParam, Debug)]
pub(crate) struct BuildView<'w, 's> {
    tick: Res<'w, SimTick>,
    index: Res<'w, EntityIndex>,
    book: Res<'w, ActionBook>,
    builds: Res<'w, BuildSpecs>,
    requirements: Res<'w, Requirements>,
    metric: Res<'w, Metric>,
    resources: Option<Res<'w, PlayerResources>>,
    modifiers: Option<Res<'w, PlayerModifiers>>,
    placement: Placement<'w, 's>,
    builders: Query<
        'w,
        's,
        (
            Entity,
            &'static StableId,
            &'static Position,
            Option<&'static Body>,
            &'static Team,
            Option<&'static Owner>,
            &'static ActionSlots,
            Option<&'static Pools>,
            &'static Builder,
            Option<&'static Destination>,
            Option<&'static Route>,
            Option<&'static UnitTags>,
        ),
        Without<Dead>,
    >,
    sites: Query<
        'w,
        's,
        (
            &'static Position,
            &'static Body,
            &'static UnitType,
            Option<&'static Owner>,
            &'static Site,
        ),
        Without<Dead>,
    >,
    owned: Query<'w, 's, (&'static UnitType, &'static Owner), (Without<Dead>, Without<Site>)>,
    changed: Query<'w, 's, Entity, (Changed<Builder>, Without<Dead>)>,
}

/// A builder that builds a site this tick, as the Mode stage finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BuildingAt {
    site: StableId,
    builder: StableId,
}

/// What a builder does with its build order this tick.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// It walks to the point of its target's box nearest it.
    Walk(Position),
    /// It stays: a builder of its site, in range.
    Build,
    /// It takes its `builder` site, of this entity, which no other builder holds, and builds it.
    Take(Entity),
    /// Its order ends.
    End,
    /// Its build starts, its site spawning.
    Start(Start),
}

/// A build that starts: the building's type and place, and what it pays and spends.
#[derive(Debug, Clone, Copy)]
struct Start {
    unit_type: UnitType,
    at: Position,
    angle: Num,
    cost: PoolCost,
    cooldown: Ticks,
    rank: u8,
}

/// A builder's build of its order's slot: the building's type, its spec, and its range.
#[derive(Debug, Clone, Copy)]
struct Building<'a> {
    unit_type: UnitType,
    spec: BuildSpec<'a>,
    range: Num,
}

/// Where a build's building stands: at the order's point, at the builder's height, its box
/// turned by the order's angle, or the site's.
#[derive(Debug, Clone, Copy)]
struct Placed {
    at: Position,
    body: Body,
}

impl BuildView<'_, '_> {
    /// Fills `held` with the living, complete units the players own, by type.
    fn held(&self, held: &mut Vec<Held>) {
        let units = self.owned.iter().map(|(&unit_type, owner)| Held {
            owner: owner.slot(),
            unit_type,
        });
        Held::collect(held, units);
    }

    /// The build in `slot` of `slots`; `None` for a slot that holds no build.
    fn building(&self, slots: &ActionSlots, slot: u8) -> Option<Building<'_>> {
        let action = slots.slot(slot)?.action?;
        let KindSpec::Build(unit_type) = self.book.get(action)?.kind else {
            return None;
        };
        let Range::Meters(range) = self.book.range(slots, slot) else {
            panic!("the load checked a build's range in meters");
        };
        let spec = self
            .builds
            .of(action)
            .expect("a build's spec is in the book");
        Some(Building {
            unit_type,
            spec,
            range,
        })
    }

    /// Where the build at `target` places its building, by a builder at `from` of `owner`: the
    /// point's box, or a living site of the same building and player.
    fn placed(
        &self,
        building: Building<'_>,
        from: Position,
        owner: Option<&Owner>,
        target: BuildTarget,
    ) -> Option<Placed> {
        match target {
            BuildTarget::Point { x, z, angle } => {
                let at = Vec3::new(x, from.get().y, z);
                Some(Placed {
                    at: Position::new(at).expect("a point within the bounds"),
                    body: building.spec.form.at(angle),
                })
            }
            BuildTarget::Site(site) => {
                let (&at, &body, &unit_type, site_owner, _) =
                    self.sites.get(self.index.get(site)?).ok()?;
                let same = unit_type == building.unit_type && site_owner == owner;
                same.then_some(Placed { at, body })
            }
        }
    }

    /// The start of the build at a point of the builder of `entity`'s `order`, when it passes
    /// its action's checks, its requirements, by `held`, and its placement.
    fn start(&self, entity: Entity, order: BuildOrder, held: &[Held]) -> Option<Start> {
        let (_, _, &from, _, &team, owner, slots, pools, ..) = self.builders.get(entity).ok()?;
        let BuildTarget::Point { angle, .. } = order.target else {
            return None;
        };
        let building = self.building(slots, order.slot)?;
        let placed = self.placed(building, from, owner, order.target)?;
        let owner = owner.map(|owner| owner.slot());
        let purse = Purse {
            pools,
            resources: self.resources.as_deref(),
            owner,
        };
        let aim = SlotAim {
            slot: order.slot,
            target: ActionTarget::Point(placed.at),
        };
        let no_target = |_| Attitude::Friendly;
        let checked = self
            .book
            .check(self.tick.start(), slots, purse, aim, no_target, |_| None)?;
        let holdings = Holdings {
            units: held,
            modifiers: self.modifiers.as_deref(),
        };
        let met = holdings.meet(owner, self.requirements.of(checked.id));
        let room = self
            .placement
            .passes(team, placed.at, placed.body, building.spec);
        (met && room).then_some(Start {
            unit_type: building.unit_type,
            at: placed.at,
            angle,
            cost: checked.values.cost,
            cooldown: checked.values.cooldown,
            rank: checked.rank,
        })
    }

    /// Whether the order of the builder of `entity` passes as it applies: a point's build its
    /// checks, by `held`, a site's the site.
    fn applies(&self, entity: Entity, held: &[Held]) -> bool {
        let Ok((_, _, &from, _, _, owner, slots, _, builder, ..)) = self.builders.get(entity)
        else {
            return false;
        };
        let Some(order) = builder.order() else {
            return true;
        };
        match order.target {
            BuildTarget::Point { .. } => self.start(entity, order, held).is_some(),
            BuildTarget::Site(_) => self
                .building(slots, order.slot)
                .and_then(|building| self.placed(building, from, owner, order.target))
                .is_some(),
        }
    }

    /// Whether the builder at `from` with a body of `body` and a build of `building` builds the
    /// building `placed`: it comes within its range.
    fn in_range(
        &self,
        from: Position,
        body: Option<&Body>,
        building: Building<'_>,
        placed: Placed,
    ) -> bool {
        let shape = Body::shape_of(body);
        self.metric
            .reaches(from, shape, building.range, placed.at, placed.body.shape())
    }

    /// Whether the builder `holder` holds the site `site`: it lives, and its order is to build
    /// the site.
    fn holds(&self, holder: StableId, site: StableId) -> bool {
        let found = self
            .index
            .get(holder)
            .and_then(|unit| self.builders.get(unit).ok());
        found.is_some_and(|(.., builder, _, _, _)| {
            builder
                .order()
                .is_some_and(|order| order.target == BuildTarget::Site(site))
        })
    }

    /// What the builder of `entity` does with its order this tick: out of range of its target's
    /// box, it walks to the box's point nearest it, unless its route arrived short of that point,
    /// which ends the order; in range, a point's build starts when it passes its checks, by
    /// `held`, and ends else; a builder of a site builds when the site takes it.
    fn step(&self, entity: Entity, held: &[Held]) -> Step {
        let Ok((_, &id, &from, body, _, owner, slots, _, builder, destination, route, _)) =
            self.builders.get(entity)
        else {
            return Step::End;
        };
        let Some(order) = builder.order() else {
            return Step::End;
        };
        let Some(building) = self.building(slots, order.slot) else {
            return Step::End;
        };
        let Some(placed) = self.placed(building, from, owner, order.target) else {
            return Step::End;
        };
        if !self.in_range(from, body, building, placed) {
            let Shape::Box(boxed) = placed.body.shape() else {
                panic!("a building's body is a box");
            };
            let to = boxed.nearest_point(placed.at, from);
            return if Destination::gives_up(destination, route, to) {
                Step::End
            } else {
                Step::Walk(to)
            };
        }
        match order.target {
            BuildTarget::Point { .. } => self
                .start(entity, order, held)
                .map_or(Step::End, Step::Start),
            BuildTarget::Site(site) => {
                let site_entity = self.index.get(site).expect("a placed site stands");
                let (.., held) = self.sites.get(site_entity).expect("a placed site stands");
                match (building.spec.style, held.holder()) {
                    (Style::Alone, _) => Step::End,
                    (Style::Builders, _) => Step::Build,
                    (Style::Builder, Some(holder)) if holder == id => Step::Build,
                    (Style::Builder, Some(holder)) if self.holds(holder, site) => Step::End,
                    (Style::Builder, _) => Step::Take(site_entity),
                }
            }
        }
    }
}

impl Construction {
    /// Checks each build order that changed since the last run, in Inputs, after the orders
    /// apply, as it applies: a point's build passes its action's checks, its requirements and
    /// its placement against the match as it stands, and a site's is a living site of the same
    /// building and player; one that fails ends, and pays nothing.
    pub(crate) fn check_builds(
        mut parts: ParamSet<'_, '_, (BuildView<'_, '_>, Query<'_, '_, &mut Builder>)>,
        (mut changed, mut held): (Local<'_, Vec<Entity>>, Local<'_, Vec<Held>>),
    ) {
        changed.clear();
        {
            let view = parts.p0();
            changed.extend(view.changed.iter());
            if changed.is_empty() {
                return;
            }
            view.held(&mut held);
            changed.retain(|&entity| !view.applies(entity, &held));
        }
        let mut builders = parts.p1();
        for &entity in &*changed {
            builders
                .get_mut(entity)
                .expect("a builder read this tick")
                .set(None);
        }
    }

    /// Ends the build order of each builder that died since the last run: a dead unit takes no
    /// order, and starts no action.
    pub(crate) fn forget_dead_builds(mut builders: Query<'_, '_, &mut Builder, Added<Dead>>) {
        for mut builder in &mut builders {
            if builder.order().is_some() {
                builder.set(None);
            }
        }
    }

    /// Runs each builder's build order, in Act, by stable id, as `BuildView::step` says; a
    /// builder in range stops its walk. A build that starts pays its whole cost, goes on
    /// cooldown, and spawns its building as a site, of the builder's team and player, through the
    /// mode's spawner, which moves each walker its box overlaps out to the nearest cell it may
    /// stand in; its life starts at `start_life` of its maximum, rounded down, at least the least
    /// amount above 0. Each build sees the sites the builds before it placed. An `alone` build's
    /// order then ends; another's builds its site.
    pub(crate) fn start_builds(
        world: &mut World,
        builders: &mut QueryState<(Entity, &StableId, &Builder), Without<Dead>>,
        view: &mut SystemState<BuildView<'_, '_>>,
        (mut order, mut held, mut paid): (
            Local<'_, Ordered>,
            Local<'_, Vec<Held>>,
            Local<'_, Vec<ResourceAmount>>,
        ),
    ) {
        let ordered = builders
            .iter(world)
            .filter(|(.., builder)| builder.order().is_some())
            .map(|(entity, &id, _)| Keyed { id, entity });
        let due = order.sort(ordered);
        if due.is_empty() {
            return;
        }
        view.get(world)
            .expect("a build view is always valid")
            .held(&mut held);
        for &Keyed { entity, .. } in due {
            let step = view
                .get(world)
                .expect("a build view is always valid")
                .step(entity, &held);
            match step {
                Step::Walk(to) => Destination::go(world, entity, Some(to)),
                Step::Build => Destination::go(world, entity, None),
                Step::Take(site) => {
                    Destination::go(world, entity, None);
                    let id = *world.get::<StableId>(entity).expect("a builder has an id");
                    let mut held = world.get_mut::<Site>(site).expect("a site to take");
                    held.hold(id);
                }
                Step::End => {
                    let mut builder = world.get_mut::<Builder>(entity).expect("a builder");
                    builder.set(None);
                }
                Step::Start(start) => {
                    Destination::go(world, entity, None);
                    Construction::spawn_site(world, entity, start, &mut paid);
                }
            }
        }
    }

    /// Starts the build `start` of the builder of `entity`: pays, goes on cooldown, and spawns
    /// the site, whose build's paid player resources `paid` holds for its cancel.
    fn spawn_site(world: &mut World, entity: Entity, start: Start, paid: &mut Vec<ResourceAmount>) {
        let now = world.resource::<SimTick>().start();
        let unit = world.entity(entity);
        let team = *unit.get::<Team>().expect("a builder has a team");
        let owner = unit.get::<Owner>().map(|owner| owner.slot());
        let order = unit
            .get::<Builder>()
            .and_then(|builder| builder.order())
            .expect("a build that starts has its order");
        let action = unit
            .get::<ActionSlots>()
            .and_then(|slots| slots.slot(order.slot))
            .and_then(|slot| slot.action)
            .expect("a build's slot holds its action");
        paid.clear();
        if owner.is_some() {
            let book = world.resource::<ActionBook>();
            let cost = book.get(action).expect("a build in the book");
            paid.extend_from_slice(cost.resource_cost(start.rank));
        }
        let pay = |world: &mut World, resources: Option<&mut PlayerResources>| {
            let mut pools = world.get_mut::<Pools>(entity);
            let payer = Payer {
                pools: pools.as_deref_mut(),
                resources,
                owner,
            };
            payer.pay(&start.cost, paid);
        };
        if world.contains_resource::<PlayerResources>() {
            world.resource_scope(|world, mut resources: Mut<'_, PlayerResources>| {
                pay(world, Some(&mut resources));
            });
        } else {
            pay(world, None);
        }
        let mut slots = world
            .get_mut::<ActionSlots>(entity)
            .expect("a builder has slots");
        slots.cool_down(order.slot, now.after(start.cooldown));
        let id = world.resource_mut::<IdAllocator>().allocate();
        let at = SpawnAt {
            id,
            unit_type: start.unit_type,
            team,
            pos: start.at,
            angle: start.angle,
        };
        let spawner = world.non_send::<Spawner>().clone();
        let site = spawner.spawn(world, at, owner);
        let spec = world
            .resource::<BuildSpecs>()
            .of(action)
            .expect("a build's spec is in the book");
        let (style, start_life) = (spec.style, spec.start_life);
        let life = world.get_resource::<LifePool>().map(|life| life.0);
        let mut gain = Num::ZERO;
        if let (Some(life), Some(mut pools)) = (life, world.get_mut::<Pools>(site))
            && let Some(max) = pools.max(life)
        {
            let share = start_life.expect("the load checked a site's start life");
            let starts = share.of_num(max).max(Num::EPSILON).min(max);
            gain = max - starts;
            pools.take(life, gain);
        }
        let builder = *world.get::<StableId>(entity).expect("a builder has an id");
        let holder = (style == Style::Builder).then_some(builder);
        let held = Site::new(action, start.rank, gain, paid, holder);
        let status = StatusTags::of([EngineTag::Constructing]);
        world.entity_mut(site).insert((held, status));
        let next = (style != Style::Alone).then_some(BuildOrder {
            slot: order.slot,
            target: BuildTarget::Site(id),
        });
        world
            .get_mut::<Builder>(entity)
            .expect("a builder")
            .set(next);
    }

    /// Grows each site, in the Mode stage before the trains finish, by stable id: its progress
    /// adds its build's rate for the count of builders that build it, a `builder` site's the one
    /// that holds it alone, toward the build's time in ticks, and its life the gain of that
    /// progress, within its maximum. A builder builds a site while it lives, its order is to
    /// build it, its body is within its build's range of the site's, and no tag blocks its `use`
    /// group. A site that reaches its time completes: it loses
    /// its site and its `constructing` tag, and its builders' orders end.
    pub(crate) fn progress_sites(
        (index, book, builds, metric, life): (
            Res<'_, EntityIndex>,
            Res<'_, ActionBook>,
            Res<'_, BuildSpecs>,
            Res<'_, Metric>,
            Option<Res<'_, LifePool>>,
        ),
        mut sites: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Position,
                &Body,
                &mut Site,
                Option<&mut Pools>,
                &mut StatusTags,
            ),
            Without<Dead>,
        >,
        mut builders: Query<
            '_,
            '_,
            (
                &StableId,
                &Position,
                Option<&Body>,
                &ActionSlots,
                &mut Builder,
                Option<&UnitTags>,
            ),
            Without<Dead>,
        >,
        mut commands: Commands<'_, '_>,
        (mut building, mut order, mut done): (
            Local<'_, Vec<BuildingAt>>,
            Local<'_, Ordered>,
            Local<'_, Vec<StableId>>,
        ),
    ) {
        building.clear();
        for (&builder_id, &from, body, slots, builder, tags) in &builders {
            let Some(BuildOrder {
                slot,
                target: BuildTarget::Site(site),
            }) = builder.order()
            else {
                continue;
            };
            let Some(Ok((_, _, &at, site_body, ..))) = index.get(site).map(|unit| sites.get(unit))
            else {
                continue;
            };
            let Range::Meters(range) = book.range(slots, slot) else {
                panic!("the load checked a build's range in meters");
            };
            let reaches = metric.reaches(from, Body::shape_of(body), range, at, site_body.shape());
            if reaches && !UnitTags::properties_of(tags).blocks(Block::Use) {
                building.push(BuildingAt {
                    site,
                    builder: builder_id,
                });
            }
        }
        building.sort_unstable();
        done.clear();
        let ordered = sites.iter().map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { entity, id } in order.sort(ordered) {
            let (.., mut site, pools, mut status) =
                sites.get_mut(entity).expect("a site in the order");
            let spec = builds
                .of(site.action())
                .expect("a site's build is in the book");
            let at_site = |builder| BuildingAt { site: id, builder };
            let builders = building.partition_point(|&at| at.site <= id)
                - building.partition_point(|&at| at.site < id);
            let count = match (spec.style, site.holder()) {
                (Style::Builder, Some(holder)) => {
                    usize::from(building.binary_search(&at_site(holder)).is_ok())
                }
                (Style::Builder, None) => 0,
                (Style::Alone | Style::Builders, _) => builders,
            };
            let action = book
                .get(site.action())
                .expect("a site's build is in the book");
            let ticks = action.values(site.rank()).windup.get();
            let time = Num::from_int(i64::try_from(ticks).expect("a build's time fits"))
                .expect("a build's time fits a Num");
            let progressed = site.progress_by(spec.rate(count), time);
            if let (Some(life), Some(mut pools)) = (life.as_deref(), pools) {
                pools.add(life.0, progressed.life);
            }
            if progressed.complete {
                commands.entity(entity).remove::<Site>();
                *status = StatusTags::default();
                done.push(id);
            }
        }
        if done.is_empty() {
            return;
        }
        done.sort_unstable();
        for (_, _, _, _, mut builder, _) in &mut builders {
            let ends = builder.order().is_some_and(|order| match order.target {
                BuildTarget::Site(site) => done.binary_search(&site).is_ok(),
                BuildTarget::Point { .. } => false,
            });
            if ends {
                builder.set(None);
            }
        }
    }
}
