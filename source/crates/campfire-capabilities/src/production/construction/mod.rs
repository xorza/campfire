use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, QueryState, Without};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res, SystemState};
use bevy_ecs::world::{Mut, World};
use campfire_math::Num;
use campfire_sim::{EntityIndex, IdAllocator, Keyed, Ordered, Position, SimTick, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::payer::Payer;
use crate::geometry::metric::Metric;
use crate::navigation::destination::Destination;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::production::build_specs::{BuildSpecs, Style};
use crate::production::build_target::BuildTarget;
use crate::production::builder::{BuildOrder, Builder};
use crate::production::construction::build_view::{BuildView, Placed};
use crate::production::construction::building_at::BuildingAt;
use crate::production::construction::step::{Start, Step};
use crate::production::held::Held;
use crate::production::site::Site;
use crate::stats::life_pool::LifePool;
use crate::stats::pools::Pools;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::engine_tag::EngineTag;
use crate::units::owner::Owner;
use crate::units::spawn_at::SpawnAt;
use crate::units::spawner::Spawner;
use crate::units::status_tags::StatusTags;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;

pub(crate) mod build_view;
pub(crate) mod building_at;
pub(crate) mod step;

/// The systems of construction: a build order checked as it applies, builds that start in order
/// of their builders' stable ids, and sites that grow and complete.
#[derive(Debug)]
pub(crate) struct Construction;

impl Construction {
    /// Whether `range` from a builder at `from` with `body` reaches the building `placed`, in
    /// `metric`: from the edge of the one body to the edge of the other.
    fn in_range(
        metric: Metric,
        from: Position,
        body: Option<&Body>,
        range: Num,
        placed: Placed,
    ) -> bool {
        metric.reaches(
            from,
            Body::shape_of(body),
            range,
            placed.at,
            placed.body.shape(),
        )
    }

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
        let owner = unit.get::<Owner>().copied();
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
        let book = world.resource::<ActionBook>();
        let cost = book.get(action).expect("a build in the book");
        paid.extend_from_slice(cost.resource_cost(start.rank));
        let pay = |world: &mut World, resources: Option<&mut PlayerResources>| {
            let mut pools = world.get_mut::<Pools>(entity);
            let payer = Payer::of(pools.as_deref_mut(), resources, owner.as_ref());
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
        let site = spawner.spawn(world, at, owner.map(Owner::slot));
        let spec = world
            .resource::<BuildSpecs>()
            .of(action)
            .expect("a build's spec is in the book");
        let start_life = spec.start_life;
        let one_holds = matches!(spec.style, Style::Builder);
        let stays = !matches!(spec.style, Style::Alone);
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
        let holder = one_holds.then_some(builder);
        let held = Site::new(action, start.rank, gain, paid, holder);
        let status = world.get::<StatusTags>(site).copied().unwrap_or_default();
        let status = status.turned(EngineTag::Constructing, true);
        world.entity_mut(site).insert((held, status));
        let next = stays.then_some(BuildOrder {
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
            let range = book.meters(slots, slot);
            let placed = Placed {
                at,
                body: *site_body,
            };
            let reaches = Construction::in_range(*metric, from, body, range, placed);
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
                (Style::Alone | Style::Builders(_), _) => builders,
            };
            let action = book
                .get(site.action())
                .expect("a site's build is in the book");
            let time = action
                .windup_ticks(site.rank())
                .expect("a build's time fits a Num");
            let progressed = site.progress_by(spec.rate(count), time);
            if let (Some(life), Some(mut pools)) = (life.as_deref(), pools) {
                pools.add(life.0, progressed.life);
            }
            if progressed.complete {
                commands.entity(entity).remove::<Site>();
                *status = status.turned(EngineTag::Constructing, false);
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
