use campfire_math::{Num, Vec3};
use campfire_sim::{Capability, IdAllocator, StableId};

use super::*;
use crate::capability_set::test_match::TestMatch;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::pools::Pools;
use crate::units::tag_effects::TagEffects;
use crate::units::unit::Unit;
use crate::values::attitude::Attitude;
use crate::values::bounds::Bounds;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
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
        let limits = ScriptLimits::ROOMY;
        let scripts = ScriptBudgets::new(limits, 1);
        let declared = [Capability::Stats, Capability::Combat, Capability::Vision];
        let mut sim = TestMatch::server(&declared, scripts);
        let bounds = Bounds::new([num(-10), num(-10)], [num(10), num(10)]).unwrap();
        let grid = Grid::new(num(1), bounds).unwrap();
        Vision::load_grid(&mut sim.world, grid, teams);
        Scene { sim }
    }

    /// A unit of `team` at `x`, `z` with the life pool, so it may be a target, and seeing
    /// `sight` when given.
    fn spawn(&mut self, team: u8, x: i64, z: i64, sight: Option<i64>) -> StableId {
        let id = self.sim.world.resource_mut::<IdAllocator>().allocate();
        let life = Pools::life(num(100));
        let mut unit = self.sim.world.spawn((id, at(x, z), Team::new(team), life));
        if let Some(range) = sight {
            unit.insert(Sight::new(num(range)).unwrap());
        }
        id
    }

    /// Gives unit `id` tags with `effects`, as its modifiers would.
    fn set_effects(&mut self, id: StableId, effects: TagEffects) {
        self.sim.insert(id, UnitTags::with_effects(effects));
    }

    fn seen_by(&self, id: StableId) -> TeamSet {
        let relations = self.sim.world.resource::<Relations>();
        Vision::seen_by(&self.sim.world.entity(self.sim.entity(id)), relations)
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
    scene.set_effects(sneak, TagEffects::default().with_hidden());
    let ward = scene.spawn(0, -4, 0, Some(3));
    scene.set_effects(ward, TagEffects::default().with_detects());
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
    let names = scene.sim.state_names();
    assert!(names.contains(&"vision.seen_by") && names.contains(&"vision.sight"));
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
