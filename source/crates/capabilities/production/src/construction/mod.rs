use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, QueryState, Without};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res, SystemState};
use bevy_ecs::world::{EntityMut, World};
use campfire_actions::{ActionBook, ActionKind, ActionSlots, Payer, SlotTimes};
use campfire_declare::{ByType, RoleForm};
use campfire_geometry::Placed;
use campfire_math::Num;
use campfire_players::{PlayerResources, PlayersWorld, ResourceAmount};
use campfire_sim::{EntityIndex, IdAllocator, Keyed, Ordered, Position, SimTick, StableId};
use campfire_stats::{Life, StatsWorld, UnitPools};
use campfire_units::{Body, Dead, Destination, MapBook, Owner, SpawnAt, Spawner, StatusTags};
use campfire_values::{Team, UnitTags};
use campfire_vision::TeamSight;

use crate::Production;
use crate::build_book::{BuildBook, Style};
use crate::build_target::BuildTarget;
use crate::builder::{BuildOrder, Builder};
use crate::construction::build_view::{BuildPlace, BuildView};
use crate::construction::building_at::BuildingAt;
use crate::construction::step::{Start, Step};
use crate::site::Site;

pub(crate) mod build_view;
pub(crate) mod building_at;
pub(crate) mod step;

/// The systems of construction: a build order checked as it applies, builds that start in order
/// of their builders' stable ids, and sites that grow and complete.
#[derive(Debug)]
pub struct Construction;

impl Construction {
    /// Checks each build order that changed since the last run, in Inputs, after the orders
    /// apply, as it applies: a point's build passes its action's checks, its requirements and
    /// its placement against the match as it stands, and a site's is a living site of the same
    /// building and player; one that fails ends, and pays nothing.
    pub(crate) fn check_builds<S: TeamSight>(
        mut parts: ParamSet<'_, '_, (BuildView<'_, '_, S>, Query<'_, '_, &mut Builder>)>,
        mut changed: Local<'_, Vec<Entity>>,
    ) {
        changed.clear();
        {
            let mut view = parts.p0();
            changed.extend(view.changed.iter());
            if changed.is_empty() {
                return;
            }
            view.read_holdings();
            changed.retain(|&entity| !view.applies(entity));
            view.holdings.forget();
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
    /// order then ends; another's builds its site. On a predicting client, a build that starts
    /// spawns nothing, and its order ends until the server's comes back.
    pub(crate) fn start_builds<S: TeamSight>(
        world: &mut World,
        builders: &mut QueryState<(Entity, &StableId, &Builder), Without<Dead>>,
        view: &mut SystemState<BuildView<'_, '_, S>>,
        (mut order, mut paid): (Local<'_, Ordered>, Local<'_, Vec<ResourceAmount>>),
    ) {
        let ordered = builders
            .iter(world)
            .filter(|(.., builder)| builder.order().is_some())
            .map(|(entity, &id, _)| Keyed { id, entity });
        let due = order.sort(ordered);
        if due.is_empty() {
            return;
        }
        view.get_mut(world)
            .expect("a build view is always valid")
            .read_holdings();
        for &Keyed { entity, .. } in due {
            let step = view
                .get_mut(world)
                .expect("a build view is always valid")
                .step(entity);
            match step {
                Step::Walk(to) => Destination::go_to(world, entity, Some(to)),
                Step::Build => Destination::go_to(world, entity, None),
                Step::Take(site) => {
                    Destination::go_to(world, entity, None);
                    let id = *world.get::<StableId>(entity).expect("a builder has an id");
                    let mut site_part = world.get_mut::<Site>(site).expect("a site to take");
                    site_part.hold(id);
                }
                Step::End => {
                    let mut builder = world.get_mut::<Builder>(entity).expect("a builder");
                    builder.set(None);
                }
                Step::Start(start) => {
                    Destination::go_to(world, entity, None);
                    Self::spawn_site::<S>(world, entity, start, &mut paid);
                }
            }
        }
    }

    /// Starts the build `start` of the builder of `entity`: pays, goes on cooldown, and spawns
    /// the site, whose build's paid player resources `paid` holds for its cancel. A predicting
    /// client's form `F` goes on cooldown and ends the order alone, as the builder's pools, its
    /// player's resources and the site are the server's.
    fn spawn_site<F: RoleForm>(
        world: &mut World,
        entity: Entity,
        start: Start,
        paid: &mut Vec<ResourceAmount>,
    ) {
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
        let life = world.life_pool();
        let pay = |unit: &mut EntityMut<'_>, resources: Option<&mut PlayerResources>| {
            let pools = unit.get_components_mut::<UnitPools>();
            let pools = pools.expect("a builder's pools").of(life);
            Payer::of(pools, resources).pay(&start.cost, paid);
        };
        let player = owner.and_then(|owner| world.player_entity(owner.slot()));
        match (F::PREDICTS, player) {
            (true, _) => {}
            (false, Some(player)) => {
                let [mut unit, mut player] = world.entity_mut([entity, player]);
                let mut resources = player.get_mut::<PlayerResources>();
                pay(&mut unit, resources.as_deref_mut());
            }
            (false, None) => {
                let [mut unit] = world.entity_mut([entity]);
                pay(&mut unit, None);
            }
        }
        let mut times = world
            .get_mut::<SlotTimes>(entity)
            .expect("a builder has slot times");
        times.cool_down(order.slot, now.after(start.cooldown));
        // The site is the server's, which a client predicts no spawn of: the order ends, so the
        // build starts once, and the server's, turned to the site, comes back by replication.
        if F::PREDICTS {
            world
                .get_mut::<Builder>(entity)
                .expect("a builder")
                .set(None);
            return;
        }
        let id = world.resource_mut::<IdAllocator>().allocate();
        let at = SpawnAt {
            id,
            unit_type: start.unit_type,
            team,
            pos: start.pos,
            owner: owner.map(Owner::slot),
            angle: start.angle,
        };
        let spec = world
            .resource::<BuildBook>()
            .of(action)
            .expect("a build's spec is in the book");
        let start_life = spec.start_life;
        let one_holds = matches!(spec.style, Style::Builder);
        let stays = !matches!(spec.style, Style::Alone);
        // The site's life starts at its share of the most its type's life holds, as it spawns; a
        // mode of no life pool has no book of lives, and its site no life.
        let full = world
            .get_resource::<ByType<Life>>()
            .and_then(|lives| lives.get(start.unit_type).copied());
        let gain = full.map_or(Num::ZERO, |full| {
            let max = full.max();
            let share =
                start_life.expect("a building with life has its start life, as its book checked");
            max - share.of_num(max).max(Num::EPSILON).min(max)
        });
        let builder = *world.get::<StableId>(entity).expect("a builder has an id");
        let holder = one_holds.then_some(builder);
        let site_part = Site::new(action, start.rank, gain, paid, holder);
        let status = StatusTags::from([Production::CONSTRUCTING]);
        let spawner = world.non_send::<Spawner>().clone();
        let site = spawner.spawn(world, at, (site_part, status));
        if let Some(mut life) = world.get_mut::<Life>(site) {
            life.take(gain);
        }
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
        (index, book, builds, map): (
            Res<'_, EntityIndex>,
            Res<'_, ActionBook>,
            Res<'_, BuildBook>,
            Res<'_, MapBook>,
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
                Option<&mut Life>,
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
            let range = slots.meters_in(&book, slot);
            let placed = BuildPlace {
                pos: at.get(),
                body: *site_body,
            };
            let builder = Placed::new(from.get(), Body::shape_of(body));
            let reaches = placed.reached_by(map.metric, builder, range);
            if reaches
                && !tags.is_some_and(|tags| tags.properties.blocks(ActionKind::Build.block()))
            {
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
            let (.., mut site, life, mut status) =
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
            if let Some(mut life) = life {
                life.add(progressed.life);
            }
            if progressed.complete {
                commands.entity(entity).remove::<Site>();
                *status = status.turned(Production::CONSTRUCTING, false);
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
