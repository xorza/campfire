use campfire_math::{Num, Vec3};
use campfire_script::{Budget, ScriptHost};
use campfire_sim::{Capability, IdAllocator, StableId};

use super::*;
use crate::capability_set::test_match::TestMatch;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::internals::FailureKind;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::pools::Pools;
use crate::units::tag_properties::TagProperties;
use crate::units::unit::Unit;
use crate::values::attitude::Attitude;
use crate::values::body_box::BodyBox;
use crate::values::bounds::Bounds;

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap()
}

/// A match of teams with vision on a grid of 1 m cells from (−10, −10) to (10, 10).
#[derive(Debug)]
struct Scene {
    sim: TestMatch,
}

impl Scene {
    /// A match of three teams.
    fn new() -> Scene {
        Scene::of_teams(3)
    }

    /// A match of `teams` teams.
    fn of_teams(teams: usize) -> Scene {
        Scene::with_brush(teams, &[])
    }

    /// A match of `teams` teams whose map has the brush of `brush`'s areas, in order.
    fn with_brush(teams: usize, brush: &[Polygon]) -> Scene {
        let limits = ScriptLimits::ROOMY;
        let scripts = ScriptBudgets::new(limits, 1);
        let declared = [Capability::Stats, Capability::Combat, Capability::Vision];
        let mut sim = TestMatch::server(&declared, scripts);
        let bounds =
            Bounds::new([Num::int(-10), Num::int(-10)], [Num::int(10), Num::int(10)]).unwrap();
        let grid = Grid::new(Num::int(1), bounds).unwrap();
        Vision::load_grid(&mut sim.world, grid, brush, teams);
        Scene { sim }
    }

    /// A unit of `team` at `x`, `z` with the life pool, so it may be a target, and seeing
    /// `sight` when given.
    fn spawn(&mut self, team: u8, x: i64, z: i64, sight: Option<i64>) -> StableId {
        let id = self.sim.world.resource_mut::<IdAllocator>().allocate();
        let life = Pools::life(Num::int(100));
        let mut unit = self.sim.world.spawn((id, at(x, z), Team::new(team), life));
        if let Some(range) = sight {
            unit.insert(Sight::new(Num::int(range)).unwrap());
        }
        id
    }

    /// Gives unit `id` tags with `properties`, as its modifiers would.
    fn set_properties(&mut self, id: StableId, properties: TagProperties) {
        self.sim.insert(id, UnitTags::with_properties(properties));
    }

    /// Runs `ctx.reveal(at.pos, radius, ms)` as `actor` thinks, or as the mode with no actor,
    /// then applies what it queued; why it failed, when it did.
    fn reveal(
        &mut self,
        actor: Option<StableId>,
        at: StableId,
        radius: &str,
        ms: i64,
    ) -> Result<(), FailureKind> {
        let world = &mut self.sim.world;
        let ctx = world.non_send::<Ctx>().clone();
        ctx.view().read(world);
        match actor {
            Some(actor) => ctx.frame().begin_think(world, actor),
            None => ctx.frame().begin_mode(world, false),
        }
        let at = ctx.view().unit(at).unwrap();
        let source = format!("fn probe(ctx, at) {{ ctx.reveal(at.pos, {radius}, {ms}) }}");
        let returned = {
            let mut host = world.non_send_mut::<ScriptHost>();
            let script = host.compile(&source).unwrap();
            let mut budget = Budget::new(u64::MAX);
            host.call(&mut budget, script, "probe", (ctx.clone(), at))
        };
        let returned = returned.map_err(|error| CallError::from_script(error).kind())?;
        assert!(returned.is_unit());
        let now = self.sim.now();
        ctx.apply(&mut self.sim.world, now);
        Ok(())
    }

    fn seen_by(&self, id: StableId) -> TeamSet {
        let relations = self.sim.world.resource::<Relations>();
        let parts = (self.sim.try_get::<SeenBy>(id), self.sim.try_get::<Team>(id));
        Vision::seen_by(parts, relations)
    }
}

#[test]
fn each_team_sees_the_cells_its_living_units_reveal() {
    let mut scene = Scene::new();
    // Team 0 sees 3 m from (0, 0). The cell of (2, 0) has its center at (2.5, 0.5), √6.5 ≈ 2.55 m
    // away; the cell of (3, 0), at (3.5, 0.5), √12.5 ≈ 3.54 m away.
    let seer = scene.spawn(0, 0, 0, Some(3));
    let near = scene.spawn(1, 2, 0, Some(0));
    let far = scene.spawn(1, 3, 0, None);
    // A dead unit reveals nothing, however far it sees.
    let dead = scene.spawn(2, 5, 5, Some(20));
    scene.sim.insert(dead, Dead);
    // An enemy with no life pool, as a projectile is, is no target: no query finds it.
    let shell = scene.spawn(1, 1, 0, None);
    let entity = scene.sim.entity(shell);
    scene.sim.world.entity_mut(entity).remove::<Pools>();
    let team = |index| TeamSet::of(Team::new(index));

    // Before the first Vision stage each unit is seen by its vision group: here its team alone.
    assert_eq!(scene.seen_by(near), team(1));
    scene.sim.step();
    let teams = [seer, near, far, dead].map(|id| scene.seen_by(id));
    assert_eq!(
        teams,
        [team(0), team(1).with(Team::new(0)), team(1), team(2)]
    );
    assert!(scene.seen_by(shell).contains(Team::new(0)));

    // Seen through the view: `find` returns both enemies within 10 m, `find_visible` and
    // `nearest_visible` only the one team 0 sees, and `can_see` asks the other's teams.
    let read = |scene: &mut Scene, expression: &str, of| {
        Unit::ids(scene.sim.read(expression, of).unwrap())
    };
    let find = r#"ctx.find(of, of.pos, 10, "enemies")"#;
    assert_eq!(read(&mut scene, find, seer), [near, far]);
    let visible = r#"ctx.find_visible(of, of.pos, 10, "enemies")"#;
    assert_eq!(read(&mut scene, visible, seer), [near]);
    let nearest = r#"ctx.nearest_visible(of, 10, "enemies")"#;
    assert_eq!(read(&mut scene, nearest, seer), [near]);
    assert_eq!(read(&mut scene, nearest, far), [] as [StableId; 0]);
    // Over itself, near and far, by stable id: the seer sees itself and near.
    let can_see = r#"let seen = [];
        for unit in ctx.find(of, of.pos, 10, "all") { seen.push(of.can_see(unit)); }
        seen"#;
    let flags = scene.sim.read(can_see, seer).unwrap().into_array().unwrap();
    let flags: Vec<bool> = flags
        .into_iter()
        .map(|flag| flag.as_bool().unwrap())
        .collect();
    assert_eq!(flags, [true, true, false]);

    // The seer steps 1 m: far's cell, now √6.5 m away, comes into sight in the next Vision stage.
    let entity = scene.sim.entity(seer);
    *scene.sim.world.get_mut::<Position>(entity).unwrap() = at(1, 0);
    assert_eq!(scene.seen_by(far), team(1));
    scene.sim.step();
    assert_eq!(scene.seen_by(far), team(1).with(Team::new(0)));

    // A stealthed unit of team 1 at (−1, 0), in the seer's sight, √2.5 ≈ 1.58 m from its cell's
    // center at (−0.5, 0.5); a ward of team 0 in the true sight state at (−4, 0), seeing 3 m,
    // √12.5 ≈ 3.54 m from that center. The seer's sight does not show it, nor does the ward's
    // true sight, beyond: team 1 alone sees it. The ward steps to (−3, 0), √6.5 ≈ 2.55 m off:
    // its true sight shows the sneak to team 0.
    let sneak = scene.spawn(1, -1, 0, None);
    scene.set_properties(sneak, TagProperties::default().with_hidden());
    let ward = scene.spawn(0, -4, 0, Some(3));
    scene.set_properties(ward, TagProperties::default().with_detects());
    scene.sim.step();
    assert_eq!(scene.seen_by(sneak), team(1));
    let entity = scene.sim.entity(ward);
    *scene.sim.world.get_mut::<Position>(entity).unwrap() = at(-3, 0);
    scene.sim.step();
    assert_eq!(scene.seen_by(sneak), team(1).with(Team::new(0)));

    // A new unit is seen by its whole group before its first Vision stage, as that stage gives
    // it: teams 1 and 2 become friends that share vision.
    let relations = &mut scene.sim.world.resource_mut::<Relations>();
    relations.set(Team::new(1), Team::new(2), Attitude::Friendly, true);
    let new = scene.spawn(2, 9, 9, None);
    assert_eq!(scene.seen_by(new), team(2).with(Team::new(1)));
    scene.sim.step();
    assert_eq!(scene.seen_by(new), team(2).with(Team::new(1)));

    // What a team sees is state, restored with the rest.
    let mut restored = Scene::new();
    scene.sim.restore_into(&mut restored.sim);
    assert_eq!(restored.seen_by(new), team(2).with(Team::new(1)));
}

#[test]
fn friendly_teams_share_vision_as_one_group_unless_their_vision_is_off() {
    // A match of 64 one-unit teams, the most a map with vision holds. Team 0 at the origin sees
    // 3 m; team 40 at (6, 0) sees 2 m, so neither sees the other's cell; team 63 at (−2, 0), in
    // team 0's sight, sees nothing; team 1, far off at (9, 9), sees nothing either.
    let mut scene = Scene::of_teams(64);
    let units = [
        (0, 0, 0, Some(3)),
        (40, 6, 0, Some(2)),
        (63, -2, 0, None),
        (1, 9, 9, None),
    ]
    .map(|(team, x, z, sight)| scene.spawn(team, x, z, sight));
    let [zero, forty, sixty_three, one] = units;
    // 0 and 40 friends that share vision, 0 and 63 friends with vision off.
    let mut relations = scene.sim.world.resource_mut::<Relations>();
    relations.set(Team::new(0), Team::new(40), Attitude::Friendly, true);
    relations.set(Team::new(63), Team::new(0), Attitude::Friendly, false);
    let teams = |list: &[u8]| {
        list.iter()
            .fold(TeamSet::NONE, |set, &team| set.with(Team::new(team)))
    };
    scene.sim.step();
    // 0 and 40 see as one: each is seen by both, and 63, in 0's sight, by both and itself.
    assert_eq!(scene.seen_by(zero), teams(&[0, 40]));
    assert_eq!(scene.seen_by(forty), teams(&[0, 40]));
    assert_eq!(scene.seen_by(sixty_three), teams(&[0, 40, 63]));
    assert_eq!(scene.seen_by(one), teams(&[1]));

    // With vision off between 0 and 40, as between 0 and 63, each team sees alone.
    let mut relations = scene.sim.world.resource_mut::<Relations>();
    relations.set(Team::new(0), Team::new(40), Attitude::Friendly, false);
    scene.sim.step();
    assert_eq!(scene.seen_by(zero), teams(&[0]));
    assert_eq!(scene.seen_by(forty), teams(&[40]));
    assert_eq!(scene.seen_by(sixty_three), teams(&[0, 63]));
}

#[test]
fn a_reveal_shows_its_cells_to_the_caster_group_alone_for_its_time_and_no_hidden_unit() {
    let mut scene = Scene::new();
    let team = |index| TeamSet::of(Team::new(index));
    // Team 0's caster far off sees nothing near (5, 0). Its reveal of 2 m round (5, 0) holds the
    // cell of (5, 0), its center at (5.5, 0.5) √0.5 ≈ 0.71 m away, and of (5, 1), at (5.5, 1.5)
    // √2.5 ≈ 1.58 m away; not that of (7, 0), at (7.5, 0.5) √6.5 ≈ 2.55 m away. Team 2 sees
    // nothing there, and team 1's sneak at (5, 1) stays hidden.
    let caster = scene.spawn(0, -9, -9, Some(1));
    let target = scene.spawn(1, 5, 0, None);
    let sneak = scene.spawn(1, 5, 1, None);
    scene.set_properties(sneak, TagProperties::default().with_hidden());
    let beyond = scene.spawn(1, 7, 0, None);
    scene.spawn(2, -9, 9, Some(1));
    scene.sim.step();
    assert_eq!(scene.seen_by(target), team(1));

    // 100 ms at 30 ticks a second is 3 ticks: the Vision stages of ticks 1, 2 and 3.
    assert_eq!(scene.sim.now().get(), 1);
    assert_eq!(scene.reveal(Some(caster), target, "2", 100), Ok(()));
    let seen = |scene: &Scene| [target, sneak, beyond].map(|id| scene.seen_by(id));
    for tick in 1..=3 {
        scene.sim.step();
        assert_eq!(
            seen(&scene),
            [team(1).with(Team::new(0)), team(1), team(1)],
            "tick {tick}"
        );
        // The reveal under way is state, restored with the rest, and its restored match sees
        // the same in the next tick.
        if tick == 2 {
            let mut restored = Scene::new();
            scene.sim.restore_into(&mut restored.sim);
            restored.sim.step();
            assert_eq!(restored.seen_by(target), team(1).with(Team::new(0)));
        }
    }
    // Its last tick past, it reveals nothing, and is gone.
    scene.sim.step();
    assert_eq!(seen(&scene), [team(1), team(1), team(1)]);
    assert_eq!(*scene.sim.world.resource::<Reveals>(), Reveals::default());

    // The mode's call, which has no acting unit, fails, as do a negative radius, a time of 0 and a
    // negative one.
    let fails = [
        (None, "2", 100, ApiError::NotForRole),
        (Some(caster), "-1", 100, ApiError::NegativeRadius),
        (Some(caster), "2", 0, ApiError::ZeroTime),
        (Some(caster), "2", -1, ApiError::NegativeTime),
    ];
    for (actor, radius, ms, error) in fails {
        let failed = scene.reveal(actor, target, radius, ms);
        assert_eq!(failed, Err(FailureKind::Api(error)), "{error:?}");
    }
    assert_eq!(*scene.sim.world.resource::<Reveals>(), Reveals::default());
}

#[test]
fn a_unit_in_brush_is_seen_only_from_its_brush_and_by_a_reveal() {
    // Brush A from (2, −2) to (6, 2) holds the 1 m cells whose centers have x from 2.5 to 5.5 and z
    // from −1.5 to 1.5; brush B from (5, −2) to (9, 2) those from 6.5 to 8.5, as the column of
    // 5.5, which both hold, is A's, the first the map lists.
    let area = |[x0, z0]: [i64; 2], [x1, z1]: [i64; 2]| {
        let corners = [[x0, z0], [x1, z0], [x1, z1], [x0, z1]].map(|point| point.map(Num::int));
        Polygon::new(corners.to_vec()).unwrap()
    };
    let brush = [area([2, -2], [6, 2]), area([5, -2], [9, 2])];
    let mut scene = Scene::with_brush(3, &brush);
    let team = |index| TeamSet::of(Team::new(index));
    let and = |a: u8, b: u8| team(a).with(Team::new(b));
    // Team 1 hides in A at (3, 0), its cell's center (3.5, 0.5); team 0 watches from (0, 0), 3.54 m
    // off, and team 2 lurks in B at (8, 0), 5 m off. Each sees 6 m.
    let hider = scene.spawn(1, 3, 0, Some(6));
    let watcher = scene.spawn(0, 0, 0, Some(6));
    let lurker = scene.spawn(2, 8, 0, Some(6));
    scene.sim.step();
    // The hider is seen by no one outside A; it sees out of A, so the watcher is seen by it, and
    // not into B, so the lurker is not. The watcher sees no brush cell at all.
    let seen = |scene: &Scene| [hider, watcher, lurker].map(|id| scene.seen_by(id));
    assert_eq!(seen(&scene), [team(1), and(0, 1), team(2)]);
    // A unit of team 2 at (5, 0), in the column both hold, stands in A, and so sees the hider, 2 m
    // off.
    scene.spawn(2, 5, 0, Some(3));
    scene.sim.step();
    assert_eq!(seen(&scene)[0], and(1, 2));
    // The watcher's reveal of 1 m round the hider sees into A for its 3 ticks.
    assert_eq!(scene.reveal(Some(watcher), hider, "1", 100), Ok(()));
    for _ in 0..3 {
        scene.sim.step();
        assert_eq!(seen(&scene)[0], and(1, 2).with(Team::new(0)));
    }
    scene.sim.step();
    assert_eq!(seen(&scene)[0], and(1, 2));
    // Once the watcher stands in A too, at (2, 1), its cell's center (2.5, 1.5), it sees the hider
    // from there.
    *scene.sim.get_mut::<Position>(watcher) = at(2, 1);
    scene.sim.step();
    assert_eq!(seen(&scene)[0], and(1, 2).with(Team::new(0)));
}

#[test]
fn a_box_is_seen_and_detected_by_any_cell_it_covers() {
    // A box of 6 by 2 m at (5, 0) covers the 1 m cells from column 2 to 7 and rows -1 and 0; its
    // own cell, (5, 0), lies 5.5 m and more from (-1, 0). A seer of team 0 there sees 4 m: the
    // center of cell (2, 0), (2.5, 0.5), lies √12.5 ≈ 3.54 m off, in sight, and that of (3, 0),
    // (3.5, 0.5), √20.5 ≈ 4.53 m off, out of it. So team 0 sees the box by its edge's cells alone,
    // where a unit seen by the cell of its position would stay hidden.
    let mut scene = Scene::new();
    let team = |index| TeamSet::of(Team::new(index));
    let building = scene.spawn(1, 5, 0, None);
    let body = BodyBox::new([Num::int(6), Num::int(2)], Num::ZERO).unwrap();
    scene.sim.insert(building, Body::boxed(body));
    let seer = scene.spawn(0, -1, 0, Some(4));
    scene.sim.step();
    assert_eq!(scene.seen_by(building), team(1).with(Team::new(0)));
    // A script's query reaches the box from its edge too, 3 m from the seer: within 3 m, not
    // within 3 m less a bit. Its radius reads `()`, as a box has none.
    let read = |scene: &mut Scene, expression: &str, of| scene.sim.read(expression, of).unwrap();
    let find = |radius: &str| format!(r#"ctx.find(of, of.pos, {radius}, "enemies")"#);
    assert_eq!(Unit::ids(read(&mut scene, &find("3"), seer)), [building]);
    let short = find("3 - num(1) / 16777216");
    assert_eq!(
        Unit::ids(read(&mut scene, &short, seer)),
        [] as [StableId; 0]
    );
    assert!(read(&mut scene, "of.radius", building).is_unit());
    // A step west, to (-2, 0), and the edge's centers lie √20.5 m off: no cell of it is seen.
    let entity = scene.sim.entity(seer);
    *scene.sim.world.get_mut::<Position>(entity).unwrap() = at(-2, 0);
    scene.sim.step();
    assert_eq!(scene.seen_by(building), team(1));
    // Hidden, it is seen only by detection: a detector of team 0 at (-1, 0), seeing 4 m, detects
    // the edge's cells, and the seer, which does not detect, back there too, does not.
    scene.set_properties(building, TagProperties::default().with_hidden());
    *scene.sim.world.get_mut::<Position>(entity).unwrap() = at(-1, 0);
    scene.sim.step();
    assert_eq!(scene.seen_by(building), team(1));
    let ward = scene.spawn(0, -1, 1, Some(4));
    scene.set_properties(ward, TagProperties::default().with_detects());
    scene.sim.step();
    assert_eq!(scene.seen_by(building), team(1).with(Team::new(0)));
}
