use campfire_codec::Toml;
use campfire_sim::EntityIndex;
use serde::Deserialize;

use super::*;
use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::geometry::polygon::Polygon;
use crate::navigation::Navigation;
use crate::navigation::walker::Walker;
use crate::navigation::wall::Wall;
use crate::orders::order::Action;
use crate::production::build_book::{BuildBook, NewBuild, PlacementCheck, Style};
use crate::production::build_target::BuildTarget;
use crate::production::builder::Builder;
use crate::production::site::Site;
use crate::stats::life::Life;
use crate::units::body::Body;
use crate::units::filter::Filter;
use crate::units::layer::Layer;
use crate::units::map_book::MapBook;
use crate::units::status_tags::StatusTags;
use crate::units::team_set::TeamSet;
use crate::values::relation_set::RelationSet;
use crate::values::share::Share;
use crate::vision::Vision;
use crate::vision::seen_by::SeenBy;

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
    Toml::parse::<Field>(&format!("share = \"{text}\""))
        .unwrap()
        .share
}

/// How a yard's build grows and where it places.
#[derive(Debug, Clone)]
struct Rules<'a> {
    style: Style<'a>,
    near: Vec<PlacementCheck>,
    away: Vec<PlacementCheck>,
    walls: Vec<Wall>,
}

impl<'a> Rules<'a> {
    fn of(style: Style<'a>) -> Rules<'a> {
        Rules {
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
}

impl Yard {
    /// A match of production and its orders over 1 m cells from (−20, −20) to (20, 20), with its
    /// `walls`, for walkers of a half meter. Its depot is a box of 4 by 2 m of 100 life; its build,
    /// within 1 m, takes 4 ticks and 15 gold, its site starting at 0.1 of its life, a cancel giving
    /// back 0.5. Player 0 holds 100 gold.
    fn place(game: &mut Match, shop: Shop, rules: Rules<'_>) -> Yard {
        let depot = shop.depot;
        let form = BodyForm::box_sized([Num::int(4), Num::int(2)]).unwrap();
        Shop::give(game, depot, form);
        Shop::give(game, depot, Life::full(Num::int(100)));
        let bounds = Bounds::new([Num::int(-20); 2], [Num::int(20); 2]).unwrap();
        game.resource_mut::<MapBook>().bounds = bounds;
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::HALF,
        };
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        Navigation::load_walls(game, &rules.walls);
        Navigation::load_pathing(game, grid, [walker]);
        let gold = shop.gold;
        let player = PlayerSlot::new(0);
        Players::spawn(game, 1, 1);
        game.player_resources_mut(player)
            .unwrap()
            .add(gold, 100)
            .unwrap();
        let cost = ResourceAmount {
            resource: gold,
            amount: 15,
        };
        let build = Actions::add_build(game, depot, Num::ONE, Ticks::new(4), Some(cost));
        let mut builds = BuildBook::default();
        let new = NewBuild {
            form,
            style: rules.style,
            start_life: Some(share("0.1")),
            refund: share("0.5"),
            near: rules.near,
            away: rules.away,
        };
        builds.push(build, new);
        game.insert_resource(builds);
        Yard { build }
    }

    /// A builder of `team`, owned by player `owner`, at `pos`, of a half meter, that walks a
    /// meter a tick, with the build in its slot 0.
    fn builder(
        self,
        game: &mut World,
        pos: impl Into<Position>,
        team: Team,
        owner: u32,
    ) -> StableId {
        let pos = pos.into();
        let build = self.build;
        let slots = ActionSlots::new([(build, SlotKind::new(0), Rank::new(1))]).timed();
        let parts = (
            Owner::new(PlayerSlot::new(owner)),
            Body::new(Num::HALF).unwrap(),
            Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
            slots,
            Builder::default(),
        );
        Units::spawn(game, pos, team, parts)
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
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
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
            // A restore checks a site's progress against its build's time, 4 ticks: a progress
            // of 4 passes, one a bit past it fails.
            let site = game.part::<Site>(depot);
            let mut restored = Site::new(site.action(), site.rank(), Num::ZERO, &[], None);
            let entity = game.unit_entity(depot);
            restored.progress_by(Num::int(4), Num::int(8));
            assert!(restored.check(&game, entity));
            restored.progress_by(Num::EPSILON, Num::int(8));
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
    }
    assert_eq!(shop.gold(&game), 85);
    assert_eq!(Production::order_of(&game, builder), None);
    assert_eq!(*game.part::<Position>(builder), halves(-6, 0));
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
        let mut game = ordering_match();
        let shop = Shop::place(&mut game);
        let yard = Yard::place(&mut game, shop, Rules::of(style));
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
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Builders(&rates)));
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
fn a_builder_that_leaves_stops_its_site_and_one_that_comes_back_goes_on() {
    // A `builder` site, of a builder at (−3, 0), progresses 1 in tick 0. In tick 1 the builder
    // walks off to (−10, 0), a meter a tick: no builder, no progress. In tick 3, at (−5, 0), it is
    // ordered back, toward the open cell (−2.5, −0.5) by the box's nearest point, which is on the
    // box; it comes into range as tick 4's move ends, so that tick's Mode stage counts it: 2, 3,
    // then 4, which completes the site in tick 6.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Builder));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    let away = Action::Move {
        x: Num::int(-10),
        z: Num::ZERO,
    };
    Production::tick_ordering(&mut game, 0, builder, away);
    assert_eq!(Production::order_of(&game, builder), None);
    game.step();
    assert_eq!(*game.part::<Position>(builder), halves(-10, 0));
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
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
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
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    let [depot] = <[StableId; 1]>::try_from(shop.depots(&mut game)).unwrap();
    assert_eq!(shop.gold(&game), 85);
    Production::tick_ordering(&mut game, 1, depot, Action::CancelBuild);
    assert!(game.try_part::<Site>(depot).is_some());
    Production::tick_ordering(&mut game, 0, depot, Action::CancelBuild);
    assert_eq!(shop.gold(&game), 92);
    let index = game.resource::<EntityIndex>();
    assert_eq!(index.get(depot), None);

    // A site that dies stops, gives nothing back, and takes no cancel: it stands, dead, with no
    // refund.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
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
    Production::tick_ordering(&mut game, 0, depot, Action::CancelBuild);
    assert_eq!(
        (shop.gold(&game), Production::site(&game, depot).1),
        (85, Some(Num::ONE))
    );
}

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
    let cases: [(&str, Rules<'_>, Option<fn(&mut Match)>, i64); 6] = [
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
                let body = Body::new(Num::ONE).unwrap();
                Units::spawn(game, halves(2, 0), Team::new(2), body);
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
        let mut game = ordering_match();
        let shop = Shop::place(&mut game);
        let yard = Yard::place(&mut game, shop, rules);
        if let Some(add) = add {
            add(&mut game);
        }
        let builder = yard.builder(
            &mut game,
            Position::new(Vec3::new(Num::int(x - 3), Num::ZERO, Num::ZERO)).unwrap(),
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
    // The controls: the plain yard, and the near rule met by an enemy 1 m from the box's edge.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!((shop.depots(&mut game).len(), shop.gold(&game)), (1, 85));
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let near = Rules {
        near: vec![rule(RelationSet::Enemies)],
        ..Rules::of(Style::Alone)
    };
    let yard = Yard::place(&mut game, shop, near);
    let enemy = Body::new(Num::HALF).unwrap();
    Units::spawn(&mut game, halves(0, 6), Team::new(1), enemy);
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!(shop.depots(&mut game).len(), 1);
}

#[test]
fn walkers_in_the_box_move_out_to_the_nearest_open_cell_and_a_hidden_enemy_does_not_refuse() {
    // A hidden enemy at (0.5, 0.5), seen by its own team alone, and a friendly walker at
    // (−0.5, −0.5), both in the box from (−2, −1) to (2, 1). The build passes, and each goes to
    // the nearest cell a walker of a half meter stands in, its center 0.5 m from the box: (0.5,
    // 1.5) and (−0.5, −1.5), a meter off each.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
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
    let at = |id| *game.part::<Position>(id);
    assert_eq!([at(hidden), at(friend)], [halves(1, 3), halves(-1, -3)]);

    // On a predicting client, an enemy walker in the box that it holds as the server sent it
    // refuses, though no team's vision sees it, which passes on the server, as the hidden enemy
    // above: the client holds what its team sees, where the server last had it.
    let mut game = Match::predicting(&ORDERING);
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
    let seen = Some(TeamSet::NONE);
    let enemy = Production::yard_walker(&mut game, halves(1, 1), Team::new(1), seen);
    game.insert_parts(enemy, Unpredicted);
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
    let pressed = Production::yard_walker(
        &mut game,
        Position::new(inside).unwrap(),
        Team::new(0),
        None,
    );
    let builder = yard.builder(&mut game, halves(-6, 0), Team::new(0), 0);
    Production::build_at(&mut game, builder, 0, 0);
    assert_eq!(*game.part::<Position>(pressed), halves(3, 3));
}

#[test]
fn two_builders_that_arrive_at_one_place_in_one_tick_place_one_site() {
    // Both in range of the origin, ordered in one tick: the lower stable id's build starts, and
    // the other's placement meets its site, which ends its order with nothing paid.
    let mut game = ordering_match();
    let shop = Shop::place(&mut game);
    let yard = Yard::place(&mut game, shop, Rules::of(Style::Alone));
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
    assert_eq!(spawned[0].owner, Some(PlayerSlot::new(0)));
}
