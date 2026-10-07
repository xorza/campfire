use campfire_sim::{EntityIndex, TickInput, TickInputs};
use serde::Deserialize;

use super::*;
use crate::navigation::Navigation;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::orders::order::{Action, Order};
use crate::production::build_specs::{BuildSpecs, NewBuild, PlacementCheck, Style};
use crate::production::build_target::BuildTarget;
use crate::production::builder::Builder;
use crate::production::site::Site;
use crate::stats::move_step::MoveStep;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::units::body::{Body, BodyForm};
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::layer::Layer;
use crate::units::status_tags::StatusTags;
use crate::units::team_set::TeamSet;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;
use crate::values::polygon::Polygon;
use crate::values::relation::Relation;
use crate::values::share::Share;
use crate::vision::seen_by::SeenBy;

/// The point at `x` and `z` half meters.
fn half(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::HALF * x, Num::ZERO, Num::HALF * z)).unwrap()
}

/// A build of slot 0 at (`x`, `z`), its box unturned.
fn at_point(x: i64, z: i64) -> Action {
    let target = BuildTarget::Point {
        x: Num::int(x),
        z: Num::int(z),
        angle: Num::ZERO,
    };
    Action::Build { slot: 0, target }
}

/// A build of slot 0 at the site `site`.
const fn at_site(site: StableId) -> Action {
    Action::Build {
        slot: 0,
        target: BuildTarget::Site(site),
    }
}

/// The share `text` writes.
fn share(text: &str) -> Share {
    #[derive(Debug, Deserialize)]
    struct Field {
        share: Share,
    }
    toml::from_str::<Field>(&format!("share = \"{text}\""))
        .unwrap()
        .share
}

/// A match of production and its orders over 1 m cells from (−20, −20) to (20, 20), with its
/// `walls`, for walkers of a half meter. Its depot is a box of 4 by 2 m of 100 life; its build,
/// within 1 m, takes 4 ticks and 15 gold, its site starting at 0.1 of its life, a cancel giving
/// back 0.5. Player 0 holds 100 gold.
#[derive(Debug)]
struct Yard {
    shop: Shop,
    build: ActionId,
    gold: ResourceId,
    player: PlayerSlot,
}

/// How a yard's build grows and where it places.
#[derive(Debug, Clone)]
struct Rules {
    style: Style,
    rates: Vec<Num>,
    near: Vec<PlacementCheck>,
    away: Vec<PlacementCheck>,
    walls: Vec<Wall>,
}

impl Rules {
    fn of(style: Style) -> Rules {
        Rules {
            style,
            rates: Vec::new(),
            near: Vec::new(),
            away: Vec::new(),
            walls: Vec::new(),
        }
    }
}

impl Yard {
    fn new(rules: Rules) -> Yard {
        let mut shop = Shop::ordering();
        let depot = shop.depot;
        let form = BodyForm::boxed([Num::int(4), Num::int(2)]).unwrap();
        let log = Rc::clone(&shop.spawned);
        let world = &mut shop.sim.world;
        world.insert_non_send(Spawner::new(move |world, at, owner| {
            log.borrow_mut().push(Spawned { at, owner });
            let mut unit = world.spawn((at.id, at.unit_type, at.team, at.pos));
            if let Some(owner) = owner {
                unit.insert(Owner::new(owner));
            }
            let entity = unit.id();
            if at.unit_type == depot {
                unit.insert((form.at(at.angle), Pools::life(Num::int(100))));
                Navigation::make_room(world, entity);
            }
            entity
        }));
        let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
        world.insert_resource(bounds);
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::HALF,
        };
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        Navigation::load_pathing(world, grid, &rules.walls, vec![walker]);
        let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
        let player = PlayerSlot::new(0);
        let mut resources = PlayerResources::new(1, 1);
        resources.add(player, gold, 100).unwrap();
        world.insert_resource(resources);
        let cost = ResourceAmount {
            resource: gold,
            amount: 15,
        };
        let build = internals::build(world, depot, Num::ONE, Ticks::new(4), Some(cost));
        let mut builds = BuildSpecs::default();
        let new = NewBuild {
            form,
            style: rules.style,
            rates: &rules.rates,
            start_life: Some(share("0.1")),
            refund: share("0.5"),
            near: rules.near,
            away: rules.away,
        };
        builds.push(build, new);
        world.insert_resource(builds);
        Yard {
            shop,
            build,
            gold,
            player,
        }
    }

    /// A builder of `team`, owned by player `owner`, at `at`, of a half meter, that walks a meter
    /// a tick, with the build in its slot 0.
    fn builder(&mut self, at: Position, team: u8, owner: u32) -> StableId {
        let slots = ActionSlots::new([(self.build, SlotKind::new(0), 1)]);
        let parts = (
            Team::new(team),
            Owner::new(PlayerSlot::new(owner)),
            Body::new(Num::HALF).unwrap(),
            Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
            slots,
            Builder::default(),
        );
        self.shop.sim.spawn(at, parts)
    }

    /// A walker of `team` at `at`, of a half meter, seen by `seen`, by all when `None`.
    fn walker(&mut self, at: Position, team: u8, seen: Option<TeamSet>) -> StableId {
        let parts = (
            Team::new(team),
            Body::new(Num::HALF).unwrap(),
            Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
        );
        let id = self.shop.sim.spawn(at, parts);
        if let Some(teams) = seen {
            self.shop.sim.insert(id, SeenBy::new(teams));
        }
        id
    }

    /// Runs a tick in which player `slot` orders `action` to `unit`.
    fn order(&mut self, slot: u32, unit: StableId, action: Action) {
        self.orders(slot, &[(unit, action)]);
    }

    /// Runs a tick in which player `slot` gives `orders`, each an action to a unit, in order.
    fn orders(&mut self, slot: u32, orders: &[(StableId, Action)]) {
        let orders: Vec<Order> = orders
            .iter()
            .map(|&(unit, action)| Order::one(unit, action))
            .collect();
        let payload = Order::payload(&orders);
        self.shop
            .sim
            .world
            .resource_mut::<TickInputs>()
            .push(TickInput {
                slot: PlayerSlot::new(slot),
                payload: &payload,
            });
        self.shop.sim.step();
    }

    /// Runs a tick in which player 0 orders `builder` to build its depot at (`x`, `z`).
    fn build_at(&mut self, builder: StableId, x: i64, z: i64) {
        self.order(0, builder, at_point(x, z));
    }

    fn gold(&self) -> i64 {
        self.shop
            .sim
            .world
            .resource::<PlayerResources>()
            .amount(self.player, self.gold)
    }

    /// The depots spawned so far, by stable id.
    fn depots(&self) -> Vec<StableId> {
        let spawned = self.shop.spawned.borrow();
        spawned
            .iter()
            .filter(|spawn| spawn.at.unit_type == self.shop.depot)
            .map(|spawn| spawn.at.id)
            .collect()
    }

    /// The depot `id`'s life, and its progress while a site.
    fn site(&self, id: StableId) -> (Num, Option<Num>) {
        let sim = &self.shop.sim;
        let life = sim.get::<Pools>(id).current(PoolId::FIRST).unwrap();
        let progress = sim.try_get::<Site>(id).map(Site::progress);
        (life, progress)
    }

    fn order_of(&self, builder: StableId) -> Option<BuildTarget> {
        self.shop
            .sim
            .get::<Builder>(builder)
            .order()
            .map(|order| order.target)
    }
}

#[test]
fn a_build_walks_into_range_pays_and_places_a_site_that_grows_to_its_life() {
    // A builder at (−10, 0) builds at the origin: the depot's box would span x −2 to 2 and z −1
    // to 1. Its build reaches 1 m from its body's edge, so from x = −3.5; it walks there a meter
    // a tick, from tick 0, and stands at −3 as tick 7 begins. Tick 7 pays 15 gold, spawns the
    // site at 10 of its 100 life, and ends the order: an `alone` site grows by itself. Its gain
    // is 90 over 4 ticks: 22.5 in each Mode stage, 32.5, 55, 77.5 and 100, which completes it in
    // tick 10.
    let mut yard = Yard::new(Rules::of(Style::Alone));
    let builder = yard.builder(half(-20, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    for _ in 1..7 {
        assert_eq!((yard.depots(), yard.gold()), (vec![], 100));
        yard.shop.tick();
    }
    let lives = [65, 110, 155, 200].map(|halves| Num::HALF * halves);
    for (tick, life) in (7..=10).zip(lives) {
        yard.shop.tick();
        let [depot] = <[StableId; 1]>::try_from(yard.depots()).unwrap();
        let progress = (tick < 10).then(|| Num::int(tick - 6));
        assert_eq!(yard.site(depot), (life, progress), "tick {tick}");
        let status = *yard.shop.sim.get::<StatusTags>(depot);
        let constructing = StatusTags::of([EngineTag::Constructing]);
        assert_eq!(
            status,
            if tick < 10 {
                constructing
            } else {
                StatusTags::default()
            }
        );
    }
    assert_eq!(yard.gold(), 85);
    assert_eq!(yard.order_of(builder), None);
    assert_eq!(*yard.shop.sim.get::<Position>(builder), half(-6, 0));
}

#[test]
fn each_style_grows_its_site_at_the_rate_of_its_builders_by_tick() {
    // A builder at (−3, 0), in range of the depot at the origin, starts it in tick 0; a second at
    // (3, 0), in range too, is ordered to the site in tick 1. Each Mode stage adds the style's
    // rate to the progress, toward 4: `alone` 1 with no builder, the second refused, as the site
    // takes none; `builder` 1, the second refused, as the site has one; `builders`, of rates 1
    // and 1.5, 1 with one, then 1.5 with two, 2.5 after tick 1, and 4 after tick 2, which
    // completes it and ends both orders.
    let one = Num::ONE;
    let cases = [
        (
            Style::Alone,
            vec![],
            [Some(one), Some(one * 2), Some(one * 3), None],
        ),
        (
            Style::Builder,
            vec![],
            [Some(one), Some(one * 2), Some(one * 3), None],
        ),
        (
            Style::Builders,
            vec![one, one + Num::HALF],
            [Some(one), Some(Num::HALF * 5), None, None],
        ),
    ];
    for (style, rates, progress) in cases {
        let mut yard = Yard::new(Rules {
            rates,
            ..Rules::of(style)
        });
        let first = yard.builder(half(-6, 0), 0, 0);
        let second = yard.builder(half(6, 0), 0, 0);
        yard.build_at(first, 0, 0);
        let [depot] = <[StableId; 1]>::try_from(yard.depots()).unwrap();
        let mut seen = vec![yard.site(depot).1];
        yard.order(0, second, at_site(depot));
        seen.push(yard.site(depot).1);
        let joined = yard.order_of(second);
        for _ in 2..4 {
            yard.shop.tick();
            seen.push(yard.site(depot).1);
        }
        assert_eq!(seen, progress, "{style:?}");
        let builds = style == Style::Builders;
        assert_eq!(
            joined,
            builds.then_some(BuildTarget::Site(depot)),
            "{style:?}"
        );
        assert_eq!(yard.order_of(second), None, "{style:?}");
        assert_eq!(yard.site(depot).0, Num::int(100), "{style:?}");
    }
}

#[test]
fn a_builder_that_leaves_stops_its_site_and_one_that_comes_back_goes_on() {
    // A `builder` site, of a builder at (−3, 0), progresses 1 in tick 0. In tick 1 the builder
    // walks off to (−10, 0), a meter a tick: no builder, no progress. In tick 3, at (−5, 0), it is
    // ordered back, toward the open cell (−2.5, −0.5) by the box's nearest point, which is on the
    // box; it comes into range as tick 4's move ends, so that tick's Mode stage counts it: 2, 3,
    // then 4, which completes the site in tick 6.
    let mut yard = Yard::new(Rules::of(Style::Builder));
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(yard.depots()).unwrap();
    let away = Action::Move {
        x: Num::int(-10),
        z: Num::ZERO,
    };
    yard.order(0, builder, away);
    assert_eq!(yard.order_of(builder), None);
    yard.shop.tick();
    assert_eq!(*yard.shop.sim.get::<Position>(builder), half(-10, 0));
    yard.order(0, builder, at_site(depot));
    let mut seen = Vec::new();
    for _ in 4..=7 {
        yard.shop.tick();
        seen.push(yard.site(depot).1);
    }
    let one = Num::ONE;
    assert_eq!(seen, [Some(one * 2), Some(one * 3), None, None]);
    assert_eq!(yard.order_of(builder), None);
}

#[test]
fn a_sites_life_ends_its_gain_above_its_start_less_the_damage_it_took() {
    // An `alone` site starts at 10 of 100 and gains 90 over 4 ticks: 32.5 after tick 0, 55 after
    // tick 1, when it takes 30, to 25; 47.5, then 70 as it completes in tick 3: 10 + 90 − 30.
    let mut yard = Yard::new(Rules::of(Style::Alone));
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(yard.depots()).unwrap();
    yard.shop.tick();
    assert_eq!(yard.site(depot).0, Num::int(55));
    yard.shop
        .sim
        .get_mut::<Pools>(depot)
        .take(PoolId::FIRST, Num::int(30));
    yard.shop.tick();
    assert_eq!(yard.site(depot), (Num::HALF * 95, Some(Num::int(3))));
    yard.shop.tick();
    assert_eq!(yard.site(depot), (Num::int(70), None));
}

#[test]
fn a_cancel_refunds_its_share_rounded_down_and_a_site_that_dies_refunds_nothing() {
    // The build paid 15 of 100 gold. Player 1's cancel does nothing; player 0's gives back 0.5 of
    // 15, 7.5 rounded down to 7, and despawns the site.
    let mut yard = Yard::new(Rules::of(Style::Alone));
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(yard.depots()).unwrap();
    assert_eq!(yard.gold(), 85);
    yard.order(1, depot, Action::CancelBuild);
    assert!(yard.shop.sim.try_get::<Site>(depot).is_some());
    yard.order(0, depot, Action::CancelBuild);
    assert_eq!(yard.gold(), 92);
    let index = yard.shop.sim.world.resource::<EntityIndex>();
    assert_eq!(index.get(depot), None);

    // A site that dies stops, and gives nothing back.
    let mut yard = Yard::new(Rules::of(Style::Alone));
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(yard.depots()).unwrap();
    yard.shop.sim.insert(depot, Dead);
    for _ in 0..4 {
        yard.shop.tick();
    }
    assert_eq!((yard.gold(), yard.site(depot).1), (85, Some(Num::ONE)));
}

#[test]
fn a_placement_refuses_walls_static_bodies_seen_enemies_the_bounds_and_its_rules() {
    // Each case's build at the origin, its box from (−2, −1) to (2, 1), by a builder in range at
    // (−3, 0), is checked as its order applies in tick 0: a refused one ends, pays nothing and
    // places nothing. Its control, the same yard without what refuses it, places its site.
    let wall = Wall {
        layer: Layer::FIRST,
        area: Polygon::new(
            [[-1, -1], [1, -1], [1, 1], [-1, 1]]
                .map(|point| point.map(Num::int))
                .to_vec(),
        )
        .unwrap(),
    };
    let rule = |relation| PlacementCheck {
        filter: Filter::of_relation(relation),
        distance: Num::int(2),
    };
    let cases: [(&str, Rules, Option<fn(&mut Yard)>, i64); 6] = [
        (
            "wall",
            Rules {
                walls: vec![wall],
                ..Rules::of(Style::Alone)
            },
            None,
            0,
        ),
        (
            "static body",
            Rules::of(Style::Alone),
            Some(|yard: &mut Yard| {
                let parts = (Team::new(2), Body::new(Num::ONE).unwrap());
                yard.shop.sim.spawn(half(2, 0), parts);
            }),
            0,
        ),
        (
            "seen enemy",
            Rules::of(Style::Alone),
            Some(|yard: &mut Yard| {
                yard.walker(half(1, 1), 1, None);
            }),
            0,
        ),
        ("bounds", Rules::of(Style::Alone), None, 19),
        (
            "near no enemy",
            Rules {
                near: vec![rule(Relation::Enemies)],
                ..Rules::of(Style::Alone)
            },
            None,
            0,
        ),
        (
            "away from no ally",
            Rules {
                away: vec![rule(Relation::Allies)],
                ..Rules::of(Style::Alone)
            },
            None,
            0,
        ),
    ];
    for (case, rules, add, x) in cases {
        let mut yard = Yard::new(rules);
        if let Some(add) = add {
            add(&mut yard);
        }
        let builder = yard.builder(
            Position::new(Vec3::new(Num::int(x - 3), Num::ZERO, Num::ZERO)).unwrap(),
            0,
            0,
        );
        yard.build_at(builder, x, 0);
        assert_eq!((yard.depots(), yard.gold()), (vec![], 100), "{case}");
        assert_eq!(yard.order_of(builder), None, "{case}");
    }
    // The controls: the plain yard, and the near rule met by an enemy 1 m from the box's edge.
    let mut yard = Yard::new(Rules::of(Style::Alone));
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    assert_eq!((yard.depots().len(), yard.gold()), (1, 85));
    let mut yard = Yard::new(Rules {
        near: vec![rule(Relation::Enemies)],
        ..Rules::of(Style::Alone)
    });
    yard.shop
        .sim
        .spawn(half(0, 6), (Team::new(1), Body::new(Num::HALF).unwrap()));
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    assert_eq!(yard.depots().len(), 1);
}

#[test]
fn walkers_in_the_box_move_out_to_the_nearest_open_cell_and_a_hidden_enemy_does_not_refuse() {
    // A hidden enemy at (0.5, 0.5), seen by its own team alone, and a friendly walker at
    // (−0.5, −0.5), both in the box from (−2, −1) to (2, 1). The build passes, and each goes to
    // the nearest cell a walker of a half meter stands in, its center 0.5 m from the box: (0.5,
    // 1.5) and (−0.5, −1.5), a meter off each.
    let mut yard = Yard::new(Rules::of(Style::Alone));
    let hidden = yard.walker(half(1, 1), 1, Some(TeamSet::of(Team::new(1))));
    let friend = yard.walker(half(-1, -1), 0, None);
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    assert_eq!(yard.depots().len(), 1);
    let at = |id| *yard.shop.sim.get::<Position>(id);
    assert_eq!([at(hidden), at(friend)], [half(1, 3), half(-1, -3)]);

    // A wall over x −1 to 1 from z 1.25 to 3 holds the centers (±0.5, 1.5) and (±0.5, 2.5): a
    // walker in the box at (0.5, 0.75) goes past it to the nearest cell clear of both, (1.5, 1.5),
    // √1.0625 m off, its center 0.5 m from the wall's edge and √0.5 m from the box's corner.
    let wall = Wall {
        layer: Layer::FIRST,
        area: Polygon::new(
            [[-4, 5], [4, 5], [4, 12], [-4, 12]]
                .map(|point| point.map(|quarter| Num::QUARTER * quarter))
                .to_vec(),
        )
        .unwrap(),
    };
    let mut yard = Yard::new(Rules {
        walls: vec![wall],
        ..Rules::of(Style::Alone)
    });
    let inside = Vec3::new(Num::HALF, Num::ZERO, Num::QUARTER * 3);
    let pressed = yard.walker(Position::new(inside).unwrap(), 0, None);
    let builder = yard.builder(half(-6, 0), 0, 0);
    yard.build_at(builder, 0, 0);
    assert_eq!(*yard.shop.sim.get::<Position>(pressed), half(3, 3));
}

#[test]
fn two_builders_that_arrive_at_one_place_in_one_tick_place_one_site() {
    // Both in range of the origin, ordered in one tick: the lower stable id's build starts, and
    // the other's placement meets its site, which ends its order with nothing paid.
    let mut yard = Yard::new(Rules::of(Style::Alone));
    let first = yard.builder(half(-6, 0), 0, 0);
    let second = yard.builder(half(6, 0), 0, 0);
    yard.orders(0, &[(second, at_point(0, 0)), (first, at_point(0, 0))]);
    assert_eq!((yard.depots().len(), yard.gold()), (1, 85));
    assert_eq!([yard.order_of(first), yard.order_of(second)], [None, None]);
    let spawned = yard.shop.spawned.borrow();
    assert_eq!(spawned[0].owner, Some(PlayerSlot::new(0)));
}
