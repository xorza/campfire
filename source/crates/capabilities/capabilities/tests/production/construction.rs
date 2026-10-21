use crate::halves;
use campfire_codec::Toml;
use campfire_geometry::{Bounds, Grid, Polygon};
use campfire_navigation::{Navigation, Walker};
use campfire_orders::OrdersDirective;
use campfire_production::internals::TestBuild;
use campfire_script::Filter;
use campfire_sim::{EntityIndex, Position};
use campfire_units::{BuildTarget, GroundPoint, Layer, Life, MapBook, SeenBy, Wall};
use campfire_values::{RelationSet, Share, TeamSet};
use campfire_vision::{Sight, Vision};
use serde::Deserialize;

use super::*;

/// A build of slot 0 at (`x`, `z`), its box unturned.
const fn at_point(x: i64, z: i64) -> OrdersDirective {
    let target = BuildTarget::Point {
        at: GroundPoint {
            x: Num::int(x),
            z: Num::int(z),
        },
        angle: Num::ZERO,
    };
    OrdersDirective::Build { slot: 0, target }
}

/// A build of slot 0 at the site `site`.
const fn at_site(site: StableId) -> OrdersDirective {
    OrdersDirective::Build {
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
    Toml::parse::<Field>(&format!("share = \"{text}\""))
        .unwrap()
        .share
}

/// How a yard's build starts, how it grows and where it places, and its depot's body.
#[derive(Debug, Clone)]
struct Rules<'a> {
    form: BodyForm,
    start: BuildStart,
    style: Style<'a>,
    near: Vec<PlacementCheck>,
    away: Vec<PlacementCheck>,
    walls: Vec<Wall>,
}

impl<'a> Rules<'a> {
    fn of(style: Style<'a>) -> Self {
        Rules {
            form: BodyForm::box_sized([Num::int(4), Num::int(2)]).unwrap(),
            start: BuildStart::Arrival,
            style,
            near: Vec::new(),
            away: Vec::new(),
            walls: Vec::new(),
        }
    }
}

/// A yard's match: a depot's build, of the shop's depot.
#[derive(Debug, Clone, Copy)]
struct Yard {
    build: ActionId,
    builder: UnitType,
}

/// A construction scene: an ordering match with the shop's and the yard's types loaded, by
/// `rules`.
#[derive(Debug)]
struct Building {
    game: Match,
    shop: Shop,
    yard: Yard,
}

impl Building {
    fn new(rules: Rules<'_>) -> Self {
        let mut game = ordering_match();
        let shop = Shop::place(&mut game);
        let yard = Yard::place(&mut game, shop, rules);
        Self { game, shop, yard }
    }
}

impl Yard {
    /// A match of production and its orders over 1 m cells from (−20, −20) to (20, 20), with its
    /// `walls`, for walkers of a half meter. Its depot is of the rules' body, a box of 4 by 2 m
    /// unless they name another, of 100 life; its build,
    /// within 1 m, takes 4 ticks and 15 gold, its site starting at 0.1 of its life, a cancel giving
    /// back 0.5. Player 0 holds 100 gold.
    fn place(game: &mut Match, shop: Shop, rules: Rules<'_>) -> Self {
        let depot = shop.depot;
        let form = rules.form;
        Shop::give(game, depot, form);
        Shop::give(game, depot, Life::full(Num::int(100)));
        let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
        MapBook::amend(game, |_, map| map.bounds = bounds);
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::HALF,
        };
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        Navigation::load_walls(game, &rules.walls);
        Navigation::load_pathing(game, grid, [walker]);
        let gold = shop.gold;
        let player = Seat::new(0);
        Players::spawn(game, 1, 1);
        game.player_resources_mut(player)
            .unwrap()
            .add(gold, 100)
            .unwrap();
        let cost = ResourceAmount {
            resource: gold,
            amount: 15,
        };
        let build = TestBuild {
            range: Num::ONE,
            time: Ticks::new(4),
            resource_cost: Some(cost),
            build: NewBuild {
                unit_type: depot,
                form,
                start: rules.start,
                style: rules.style,
                start_life: Some(share("0.1")),
                refund: share("0.5"),
                near: rules.near,
                away: rules.away,
            },
        };
        let build = Production::add_build(game, build);
        // Builders, of a half meter, walk a meter a tick.
        let builder = Units::load_type(game, TypeScope::Mode, "builder", &UnitTypeCore::default());
        Units::give_type(game, builder, BodyForm::circle(Num::HALF).unwrap());
        Units::give_type(game, builder, MoveStep::new(Num::ONE).unwrap());
        Units::give_type(game, builder, Builder::default());
        Self { build, builder }
    }

    /// A builder of `team`, owned by player `owner`, at `position`, of a half meter, that walks a
    /// meter a tick, with the build in its slot 0.
    fn builder(
        self,
        game: &mut World,
        position: impl Into<Vec3>,
        team: Team,
        owner: u32,
    ) -> StableId {
        let position = position.into();
        let build = self.build;
        let slots = TimedSlots::new(ActionSlots::new([(build, SlotKind::new(0), Rank::new(1))]));
        let parts = (Owner::new(Seat::new(owner)), slots);
        Spawn::of(self.builder, position)
            .team(team)
            .parts(parts)
            .place(game)
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
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-20, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    for _ in 1..7 {
        assert_eq!((shop.depots(&mut game), shop.gold(&game)), (vec![], 100));
        game.step();
    }
    let lives = [65, 110, 155, 200].map(|halves| Num::HALF * halves);
    for (tick, life) in (7..=10).zip(lives) {
        game.step();
        let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
        if tick == 7 {
            // A restore checks a site's time against its build's, 4 ticks, and its progress
            // against that time: a site of 4 ticks passes and one of 5 fails; a progress of 4
            // passes, and one a bit past it fails.
            let site = game.part::<Site>(depot);
            let entity = game.unit_entity(depot);
            let of_time = |time| {
                Site::new(SiteStart {
                    action: site.action(),
                    rank: site.rank(),
                    time,
                    gain: Num::ZERO,
                    paid: &[],
                    holder: None,
                })
            };
            assert_eq!(site.time(), Num::int(4));
            assert!(of_time(Num::int(4)).check(&game, entity));
            assert!(!of_time(Num::int(5)).check(&game, entity));
            let longer = of_time(Num::int(8));
            let mut restored = SiteProgress::default();
            restored.grow(&longer, Num::int(4));
            assert!(restored.check(&game, entity));
            restored.grow(&longer, Num::EPSILON);
            assert!(!restored.check(&game, entity));
        }
        let progress = (tick < 10).then(|| Num::int(tick - 6));
        assert_eq!(
            Production::site(&game, depot),
            (life, progress),
            "tick {tick}"
        );
        let status = *game.part::<StatusTags>(depot);
        let constructing = StatusTags::from([Production::CONSTRUCTING]);
        assert_eq!(
            status,
            if tick < 10 {
                constructing
            } else {
                StatusTags::default()
            }
        );
        // The site that completes joins the completed sites, which no mode answers here.
        let completed = if tick < 10 { vec![] } else { vec![depot] };
        let sites = &game.resource::<CompletedSites>().0;
        assert_eq!(sites.iter().copied().collect::<Vec<_>>(), completed);
    }
    assert_eq!(shop.gold(&game), 85);
    assert_eq!(Production::order_of(&game, builder), None);
    assert_eq!(game.part::<Position>(builder).get(), halves(-6, 0));
}

#[test]
fn a_completed_site_frees_its_units_out_of_its_exit_in_their_order() {
    // As in the first test, the depot's site completes in tick 10. A depot that frees a grunt and
    // a builder spawns both in that tick, the grunt first, of its team and player, in the cell of
    // its exit at (3.5, 0.5), as a trained unit comes out; one that frees nothing spawns none.
    for frees in [false, true] {
        let Building {
            mut game,
            shop,
            yard,
        } = Building::new(Rules::of(Style::Alone));
        let free: Box<[UnitType]> = if frees {
            Box::new([shop.grunt, yard.builder])
        } else {
            Box::new([])
        };
        let exit = FramePoint::new([Num::int(7) / Num::int(2), Num::HALF]);
        let spec = ProducerSpec {
            queue: NonZeroU8::new(1).unwrap(),
            exit,
            rally: None,
            gathers: false,
            free,
        };
        Shop::give(&mut game, shop.depot, spec);
        let builder = yard.builder(&mut game, halves(-20, 0), Team::new(0), 0);
        Production::build_at(&mut game, builder, 0, 0);
        for _ in 1..10 {
            game.step();
        }
        assert_eq!(Shop::spawned(&mut game, shop.grunt), [], "frees {frees}");
        game.step();
        let grunts = Shop::spawned(&mut game, shop.grunt);
        let builders = Shop::spawned(&mut game, yard.builder);
        let freed = Spawned {
            id: StableId::MATCH,
            team: Team::new(0),
            position: halves(7, 1),
            owner: Some(Seat::new(0)),
        };
        let place = |spawned: &Spawned| Spawned {
            id: StableId::MATCH,
            ..*spawned
        };
        if frees {
            assert_eq!(grunts.iter().map(place).collect::<Vec<_>>(), [freed]);
            assert_eq!(builders.len(), 2);
            assert_eq!(place(&builders[1]), freed);
            assert!(grunts[0].id < builders[1].id);
        } else {
            assert_eq!((grunts.len(), builders.len()), (0, 1));
        }
    }
}

#[test]
fn an_order_build_pays_and_places_its_site_as_it_applies_and_its_builder_walks_to_build_it() {
    // A builder at (−10, 0) builds at the origin as the order applies, in tick 0: it pays 15 gold
    // and the site spawns at 10 of its 100 life, 6.5 m from its range, and its order turns to the
    // site. It stands that tick, then walks a meter a tick round the site's box, which now
    // blocks its cells, and comes in range in tick 7, so the Mode stage adds a tick of progress;
    // with three more the site completes in tick 10, and the builder's order ends.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let rules = Rules {
        start: BuildStart::Order,
        ..Rules::of(Style::Builder)
    };
    let yard = Yard::place(&mut game, shop, rules);
    let builder = yard.builder(&mut game, halves(-20, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    assert_eq!(shop.gold(&game), 85);
    assert_eq!(
        Production::site(&game, depot),
        (Num::int(10), Some(Num::ZERO))
    );
    assert_eq!(
        Production::order_of(&game, builder),
        Some(BuildTarget::Site(depot))
    );
    assert_eq!(game.part::<Position>(builder).get(), halves(-20, 0));
    for tick in 1..10 {
        game.step();
        let progress = (tick >= 7).then(|| Num::int(tick - 6));
        assert_eq!(
            Production::site(&game, depot).1,
            progress.or(Some(Num::ZERO)),
            "tick {tick}"
        );
    }
    game.step();
    assert_eq!(
        Production::site(&game, depot),
        (Num::int(100), None),
        "tick 10"
    );
    assert_eq!(Production::order_of(&game, builder), None);
    assert_eq!(shop.gold(&game), 85);

    // A mode of no life pool, as one of no combat, holds no book of lives: its site spawns of no
    // life, and grows as any other.
    game.remove_resource::<ByType<Life>>();
    let lifeless = yard.builder(&mut game, halves(0, 20), Team::new(0), 0);
    Production::build_at(&mut game, lifeless, 0, 15);
    let depots = shop.depots(&mut game);
    assert_eq!((depots.len(), shop.gold(&game)), (2, 70));
    assert!(game.try_part::<Life>(depots[1]).is_none());

    // A build whose box runs past the bounds is refused as its order applies: it ends, pays
    // nothing and places nothing.
    let builder = yard.builder(&mut game, halves(-20, 10), Team::new(0), 0);
    Production::build_at(&mut game, builder, 19, 5);
    assert_eq!(Production::order_of(&game, builder), None);
    assert_eq!((shop.depots(&mut game).len(), shop.gold(&game)), (2, 70));
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
    let rates = [one, one + Num::HALF];
    let cases = [
        (
            Style::Alone,
            [Some(one), Some(one * 2), Some(one * 3), None],
        ),
        (
            Style::Builder,
            [Some(one), Some(one * 2), Some(one * 3), None],
        ),
        (
            Style::Builders(&rates),
            [Some(one), Some(Num::HALF * 5), None, None],
        ),
    ];
    for (style, progress) in cases {
        let Building {
            mut game,
            shop,
            yard,
        } = Building::new(Rules::of(style));
        let first = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
        let second = yard.builder(&mut game, halves(6, 0), Team::new(0), 0);
        Production::build_at(&mut game, first, 0, 0);
        let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
        let mut seen = vec![Production::site(&game, depot).1];
        Production::tick_ordering(&mut game, 0, second, at_site(depot));
        seen.push(Production::site(&game, depot).1);
        let joined = Production::order_of(&game, second);
        for _ in 2..4 {
            game.step();
            seen.push(Production::site(&game, depot).1);
        }
        assert_eq!(seen, progress, "{style:?}");
        let builds = matches!(style, Style::Builders(_));
        assert_eq!(
            joined,
            builds.then_some(BuildTarget::Site(depot)),
            "{style:?}"
        );
        assert_eq!(Production::order_of(&game, second), None, "{style:?}");
        assert_eq!(Production::site(&game, depot).0, Num::int(100), "{style:?}");
    }
}

#[test]
fn a_build_at_a_site_its_player_does_not_see_does_nothing() {
    // With a vision grid, the second builder's order to the site the first placed in tick 0 is
    // ignored while no team sees the site, and joins it once team 0 does.
    let rates = [Num::ONE, Num::ONE];
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Builders(&rates)));
    let first = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    let second = yard.builder(&mut game, halves(6, 0), Team::new(0), 0);
    Production::build_at(&mut game, first, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    Vision::fog_grid(&mut game, 1);
    for (seers, joined) in [
        (TeamSet::NONE, None),
        (TeamSet::of(Team::new(0)), Some(BuildTarget::Site(depot))),
    ] {
        game.insert_parts(depot, SeenBy::new(seers));
        Production::tick_ordering(&mut game, 0, second, at_site(depot));
        assert_eq!(Production::order_of(&game, second), joined, "{seers:?}");
    }
}

#[test]
fn a_build_at_a_point_its_team_does_not_see_is_refused() {
    // With vision, a build needs its point in a cell its builder's team saw in the last Vision
    // stage, as Zero Hour refuses a point under shroud. A builder at (−3, 0) of no sight sees no
    // cell: its build at the origin ends, pays nothing and places nothing. One of 4 m of sight
    // sees the origin's cell, whose center (0.5, 0.5) lies √(3.5² + 0.5²) ≈ 3.54 m off: it places
    // its site, and pays 15 of its 100 gold.
    let mut declared = ORDERING.to_vec();
    declared.push(Capability::Vision);
    for (sight, depots, gold) in [(None, 0, 100), (Some(4), 1, 85)] {
        let mut game = shop_match_of(&declared);
        let shop = Shop::place(&mut game);
        let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
        let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
        Vision::load_grid(&mut game, Grid::new(Num::ONE, bounds).unwrap(), &[], 3);
        if let Some(range) = sight {
            let sight = Sight::new(Num::int(range)).unwrap();
            Units::give_type(&mut game, yard.builder, sight);
        }
        let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
        Production::build_at(&mut game, builder, 0, 0);
        let placed = (shop.depots(&mut game).len(), shop.gold(&game));
        assert_eq!(placed, (depots, gold), "{sight:?}");
    }
}

#[test]
fn a_builder_that_leaves_stops_its_site_and_one_that_comes_back_goes_on() {
    // A `builder` site, of a builder at (−3, 0), progresses 1 in tick 0. In tick 1 the builder
    // walks off to (−10, 0), a meter a tick: no builder, no progress. In tick 3, at (−5, 0), it is
    // ordered back, toward the open cell (−2.5, −0.5) by the box's nearest point, which is on the
    // box; it comes into range as tick 4's move ends, so that tick's Mode stage counts it: 2, 3,
    // then 4, which completes the site in tick 6.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Builder));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    let away = OrdersDirective::Move {
        to: GroundPoint {
            x: Num::int(-10),
            z: Num::ZERO,
        },
    };
    Production::tick_ordering(&mut game, 0, builder, away);
    assert_eq!(Production::order_of(&game, builder), None);
    game.step();
    assert_eq!(game.part::<Position>(builder).get(), halves(-10, 0));
    Production::tick_ordering(&mut game, 0, builder, at_site(depot));
    let mut seen = Vec::new();
    for _ in 4..=7 {
        game.step();
        seen.push(Production::site(&game, depot).1);
    }
    let one = Num::ONE;
    assert_eq!(seen, [Some(one * 2), Some(one * 3), None, None]);
    assert_eq!(Production::order_of(&game, builder), None);
}

#[test]
fn a_sites_life_ends_its_gain_above_its_start_less_the_damage_it_took() {
    // An `alone` site starts at 10 of 100 and gains 90 over 4 ticks: 32.5 after tick 0, 55 after
    // tick 1, when it takes 30, to 25; 47.5, then 70 as it completes in tick 3: 10 + 90 − 30.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    game.step();
    assert_eq!(Production::site(&game, depot).0, Num::int(55));
    game.part_mut::<Life>(depot).take(Num::int(30));
    game.step();
    assert_eq!(
        Production::site(&game, depot),
        (Num::HALF * 95, Some(Num::int(3)))
    );
    game.step();
    assert_eq!(Production::site(&game, depot), (Num::int(70), None));
}

#[test]
fn a_cancel_refunds_its_share_rounded_down_and_a_site_that_dies_refunds_nothing() {
    // The build paid 15 of 100 gold. Player 1's cancel does nothing; player 0's gives back 0.5 of
    // 15, 7.5 rounded down to 7, and despawns the site.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    assert_eq!(shop.gold(&game), 85);
    Production::tick_ordering(&mut game, 1, depot, ProductionDirective::CancelBuild);
    assert!(game.try_part::<Site>(depot).is_some());
    Production::tick_ordering(&mut game, 0, depot, ProductionDirective::CancelBuild);
    assert_eq!(shop.gold(&game), 92);
    let index = game.resource::<EntityIndex>();
    assert_eq!(index.get(depot), None);

    // A site that dies stops, gives nothing back, and takes no cancel: it stands, dead, with no
    // refund.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    game.insert_parts(depot, Dead);
    for _ in 0..4 {
        game.step();
    }
    assert_eq!(
        (shop.gold(&game), Production::site(&game, depot).1),
        (85, Some(Num::ONE))
    );
    Production::tick_ordering(&mut game, 0, depot, ProductionDirective::CancelBuild);
    assert_eq!(
        (shop.gold(&game), Production::site(&game, depot).1),
        (85, Some(Num::ONE))
    );
}

/// A placement the rules refuse: its name, the rules, a change to the match before the build, and
/// the site's x, in meters.
type Case<'r> = (&'static str, Rules<'r>, Option<fn(&mut Match)>, i64);

#[test]
fn a_placement_refuses_walls_static_bodies_seen_enemies_the_bounds_and_its_rules() {
    // Each case's build at the origin, its box from (−2, −1) to (2, 1), by a builder in range at
    // (−3, 0), is checked as its order applies in tick 0: a refused one ends, pays nothing and
    // places nothing. Its control, the same yard without what refuses it, places its site.
    let wall = Wall {
        layer: Layer::FIRST,
        area: Polygon::new([[-1, -1], [1, -1], [1, 1], [-1, 1]].map(|point| point.map(Num::int)))
            .unwrap(),
    };
    let rule = |relation| PlacementCheck {
        filter: Filter::of_release_relations(relation),
        distance: Num::int(2),
    };
    let cases: [Case<'_>; 6] = [
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
            Some(|game: &mut Match| {
                let body = BodyForm::circle(Num::ONE).unwrap();
                Navigation::walker_at(game, halves(2, 0))
                    .team(Team::new(2))
                    .body(body)
                    .place(game);
            }),
            0,
        ),
        (
            "seen enemy",
            Rules::of(Style::Alone),
            Some(|game: &mut Match| {
                Production::yard_walker(game, halves(1, 1), Team::new(1), None);
            }),
            0,
        ),
        ("bounds", Rules::of(Style::Alone), None, 19),
        (
            "near no enemy",
            Rules {
                near: vec![rule(RelationSet::Enemies)],
                ..Rules::of(Style::Alone)
            },
            None,
            0,
        ),
        (
            "away from no ally",
            Rules {
                away: vec![rule(RelationSet::Allies)],
                ..Rules::of(Style::Alone)
            },
            None,
            0,
        ),
    ];
    for (case, rules, add, x) in cases {
        let Building {
            mut game,
            shop,
            yard,
        } = Building::new(rules);
        if let Some(add) = add {
            add(&mut game);
        }
        let builder = yard.builder(
            &mut game,
            Vec3::new(Num::int(x - 3), Num::ZERO, Num::ZERO),
            Team::new(0),
            0,
        );
        Production::build_at(&mut game, builder, x, 0);
        assert_eq!(
            (shop.depots(&mut game), shop.gold(&game)),
            (vec![], 100),
            "{case}"
        );
        assert_eq!(Production::order_of(&game, builder), None, "{case}");
    }
    // The control: the plain yard.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!((shop.depots(&mut game).len(), shop.gold(&game)), (1, 85));
    // An enemy at (0, 3), its edge 1.5 m from the box's, within the rules' 2 m: a rule reads it
    // only when team 0 sees it. So the near rule passes and the away rule refuses by the seen
    // enemy alone, and the hidden one, seen by its own team, is as no enemy.
    let near = || Rules {
        near: vec![rule(RelationSet::Enemies)],
        ..Rules::of(Style::Alone)
    };
    let away = || Rules {
        away: vec![rule(RelationSet::Enemies)],
        ..Rules::of(Style::Alone)
    };
    let (seen, hidden) = (TeamSet::of(Team::new(0)), TeamSet::of(Team::new(1)));
    for (case, rules, seers, depots) in [
        ("near, seen", near(), seen, 1),
        ("near, hidden", near(), hidden, 0),
        ("away, seen", away(), seen, 0),
        ("away, hidden", away(), hidden, 1),
    ] {
        let Building {
            mut game,
            shop,
            yard,
        } = Building::new(rules);
        let form = BodyForm::circle(Num::HALF).unwrap();
        let enemy = Navigation::walker_at(&mut game, halves(0, 6))
            .team(Team::new(1))
            .body(form)
            .place(&mut game);
        game.insert_parts(enemy, SeenBy::new(seers));
        let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
        Production::build_at(&mut game, builder, 0, 0);
        assert_eq!(shop.depots(&mut game).len(), depots, "{case}");
    }
}

#[test]
fn a_round_building_places_by_its_circle_and_its_builder_walks_to_its_edge() {
    // A depot of a circle of 2 m, built at the origin by a builder at (−3, 0) unless the case
    // names another x, in range of its edge at x = −2. A wall from x = 2 touches it and one a bit
    // nearer refuses it; a static body of 1 m centered at (3, 0) touches it and one at (2.5, 0)
    // refuses it; at x = 18 it reaches the bound at 20, and at x = 19 it passes it.
    let round = || Rules {
        form: BodyForm::circle(Num::int(2)).unwrap(),
        ..Rules::of(Style::Alone)
    };
    let wall = |from: Num| Rules {
        walls: vec![Wall {
            layer: Layer::FIRST,
            area: Polygon::new([
                [from, -Num::ONE],
                [Num::int(4), -Num::ONE],
                [Num::int(4), Num::ONE],
                [from, Num::ONE],
            ])
            .unwrap(),
        }],
        ..round()
    };
    let two = Num::int(2);
    // Each case's rules, the x of its static body in halves of a meter, if one, its build's x,
    // and the depots it places.
    let cases: [(&str, Rules<'_>, Option<i64>, i64, usize); 7] = [
        ("clear", round(), None, 0, 1),
        ("touching wall", wall(two), None, 0, 1),
        ("wall", wall(two - Num::EPSILON), None, 0, 0),
        ("touching body", round(), Some(6), 0, 1),
        ("body", round(), Some(5), 0, 0),
        ("at the bound", round(), None, 18, 1),
        ("past the bound", round(), None, 19, 0),
    ];
    for (case, rules, body, x, placed) in cases {
        let Building {
            mut game,
            shop,
            yard,
        } = Building::new(rules);
        if let Some(body) = body {
            Navigation::walker_at(&mut game, halves(body, 0))
                .team(Team::new(2))
                .body(BodyForm::circle(Num::ONE).unwrap())
                .place(&mut game);
        }
        let from = Vec3::new(Num::int(x - 3), Num::ZERO, Num::ZERO);
        let builder = yard.builder(&mut game, from, Team::new(0), 0);
        Production::build_at(&mut game, builder, x, 0);
        assert_eq!(shop.depots(&mut game).len(), placed, "{case}");
    }

    // From (−10, 0) its builder walks a meter a tick toward the circle's nearest point, (−2, 0),
    // and stands in range at x = −3 as tick 7 begins, as it does for a box of that edge.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(round());
    let builder = yard.builder(&mut game, halves(-20, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    for _ in 1..7 {
        assert_eq!(shop.depots(&mut game), vec![]);
        game.step();
    }
    game.step();
    assert_eq!(shop.depots(&mut game).len(), 1);
    assert_eq!(game.part::<Position>(builder).get(), halves(-6, 0));

    // A friendly walker at (0.5, 1.5) in the circle goes to the nearest cell clear of it, (0.5,
    // 2.5), its center √6.5 m from the origin, past the 2.5 m of the two radii.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(round());
    let pressed = Production::yard_walker(&mut game, halves(1, 3), Team::new(0), None);
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!(shop.depots(&mut game).len(), 1);
    assert_eq!(game.part::<Position>(pressed).get(), halves(1, 5));
}

#[test]
fn walkers_in_the_box_move_out_to_the_nearest_open_cell_and_a_hidden_enemy_does_not_refuse() {
    // A hidden enemy at (0.5, 0.5), seen by its own team alone, and a friendly walker at
    // (−0.5, −0.5), both in the box from (−2, −1) to (2, 1). The build passes, and each goes to
    // the nearest cell a walker of a half meter stands in, its center 0.5 m from the box: (0.5,
    // 1.5) and (−0.5, −1.5), a meter off each.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Alone));
    let hidden = Production::yard_walker(
        &mut game,
        halves(1, 1),
        Team::new(1),
        Some(TeamSet::of(Team::new(1))),
    );
    let friend = Production::yard_walker(&mut game, halves(-1, -1), Team::new(0), None);
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!(shop.depots(&mut game).len(), 1);
    let at = |id| game.part::<Position>(id).get();
    assert_eq!([at(hidden), at(friend)], [halves(1, 3), halves(-1, -3)]);

    // On a predicting client, an enemy walker in the box that it holds as the server sent it
    // refuses, though no team's vision sees it, which passes on the server, as the hidden enemy
    // above: the client holds what its team sees, where the server last had it.
    let mut game = Match::builder(&ORDERING).role(Role::Client).build();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
    let seen = Some(TeamSet::NONE);
    let enemy = Production::yard_walker(&mut game, halves(1, 1), Team::new(1), seen);
    game.insert_parts(enemy, Unpredicted);
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!((shop.depots(&mut game), shop.gold(&game)), (vec![], 100));
    assert_eq!(Production::order_of(&game, builder), None);

    // With no enemy in its box, the client's build starts, as the server's above places its
    // depot, and pays and spawns nothing, though the test's world holds a spawner: the site and
    // the gold are the server's. Its order ends, so it starts once; the server's comes back.
    let mut game = Match::builder(&ORDERING).role(Role::Client).build();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!((shop.depots(&mut game), shop.gold(&game)), (vec![], 100));
    assert_eq!(Production::order_of(&game, builder), None);

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
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(
        &mut game,
        shop,
        Rules {
            walls: vec![wall],
            ..Rules::of(Style::Alone)
        },
    );
    let inside = Vec3::new(Num::HALF, Num::ZERO, Num::QUARTER * 3);
    let pressed = Production::yard_walker(&mut game, inside, Team::new(0), None);
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!(game.part::<Position>(pressed).get(), halves(3, 3));
}

#[test]
fn two_builders_that_arrive_at_one_place_in_one_tick_place_one_site() {
    // Both in range of the origin, ordered in one tick: the lower stable id's build starts, and
    // the other's placement meets its site, which ends its order with nothing paid.
    let Building {
        mut game,
        shop,
        yard,
    } = Building::new(Rules::of(Style::Alone));
    let first = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    let second = yard.builder(&mut game, halves(6, 0), Team::new(0), 0);
    Production::give_orders(
        &mut game,
        0,
        &[(second, at_point(0, 0)), (first, at_point(0, 0))],
    );
    assert_eq!((shop.depots(&mut game).len(), shop.gold(&game)), (1, 85));
    assert_eq!(
        [
            Production::order_of(&game, first),
            Production::order_of(&game, second)
        ],
        [None, None]
    );
    let spawned = Shop::spawned(&mut game, shop.depot);
    assert_eq!(spawned[0].owner, Some(Seat::new(0)));
}

#[test]
fn a_predicting_clients_build_reads_only_what_it_holds() {
    // The client predicts player 0's builder and a depot site the server spawned at 10 of its 100
    // life, and holds as the server sent them an enemy walker in the depot's box and a static
    // body beside it. Its builder is ordered to build the depot. The site grows, and its life is
    // the server's to write.
    Release::sync_held(&ORDERING, 12, |game| {
        let shop = Shop::place(game);
        let yard = Yard::place(game, shop, Rules::of(Style::Alone));
        let enemy = Production::yard_walker(game, halves(1, 1), Team::new(1), None);
        game.insert_parts(enemy, Unpredicted);
        let form = BodyForm::circle(Num::HALF).unwrap();
        let post = Navigation::walker_at(game, halves(10, 0))
            .team(Team::new(2))
            .body(form)
            .place(game);
        game.insert_parts(post, Unpredicted);
        let builder = yard.builder(game, halves(-8, 0), Team::new(0), 0);
        Production::build_at(game, builder, 0, 0);
        let site = Site::new(SiteStart {
            action: yard.build,
            rank: Rank::FIRST,
            time: Num::int(4),
            gain: Num::int(90),
            paid: &[],
            holder: None,
        });
        let parts = (
            site,
            SiteProgress::default(),
            StatusTags::from([Production::CONSTRUCTING]),
            Owner::new(Seat::new(0)),
        );
        let depot = Spawn::of(shop.depot, halves(0, 12))
            .parts(parts)
            .place(game);
        game.part_mut::<Life>(depot).take(Num::int(90));
    });
}
