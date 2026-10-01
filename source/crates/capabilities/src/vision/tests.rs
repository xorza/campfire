use std::num::NonZeroU32;
use std::rc::Rc;

use campfire_math::{Num, Vec3};
use campfire_script::rhai::Dynamic;
use campfire_script::{Budget, ScriptHost};
use campfire_sim::{Capability, EntityIndex, IdAllocator, SimUpdate, StableId, TickRate, TypeHash};

use super::*;
use crate::capability_set::internals::TestMatch;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::match_scripts::MatchScripts;
use crate::scripts::script_limits::ScriptLimits;
use crate::units::tag_effects::TagEffects;
use crate::units::unit::Unit;
use crate::values::bounds::Bounds;

const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
}

/// A match of three teams with vision on a grid of 1 m cells from (−10, −10) to (10, 10).
#[derive(Debug)]
struct Scene {
    world: World,
    registry: StateRegistry,
}

impl Scene {
    fn new() -> Scene {
        let limits = ScriptLimits {
            per_call: 10_000,
            player: 100_000,
            think: 100_000,
            mode: 100_000,
        };
        let scripts = MatchScripts {
            limits,
            players: 1,
            damage_kinds: Rc::from([]),
        };
        let declared = [Capability::Combat, Capability::Vision];
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(&declared, RATE, Some(scripts));
        world.add_schedule(schedule);
        let bounds = Bounds::new([num(-10), num(-10)], [num(10), num(10)]).unwrap();
        let grid = Grid::new(num(1), bounds).unwrap();
        Vision::load_grid(&mut world, grid, 3);
        Scene { world, registry }
    }

    /// A unit of `team` at (`x`, `z`) that sees `sight` meters, if it sees.
    fn spawn(&mut self, team: u8, x: i64, z: i64, sight: Option<i64>) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let mut unit = self.world.spawn((id, at(x, z), Team::new(team)));
        if let Some(range) = sight {
            unit.insert(Sight::new(num(range)).unwrap());
        }
        id
    }

    /// Gives unit `id` tags with `effects`, as its modifiers would.
    fn set_effects(&mut self, id: StableId, effects: TagEffects) {
        let entity = self.entity(id);
        self.world
            .entity_mut(entity)
            .insert(UnitTags::with_effects(effects));
    }

    fn entity(&self, id: StableId) -> Entity {
        self.world.resource::<EntityIndex>().get(id).unwrap()
    }

    fn seen_by(&self, id: StableId) -> TeamSet {
        Vision::seen_by(&self.world.entity(self.entity(id)))
    }

    /// `probe(ctx, of)` in `source`, run on the units as they are now.
    fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError> {
        let ctx = self.world.non_send::<Ctx>().clone();
        ctx.view().read(&self.world);
        let unit = ctx.view().unit(of).unwrap();
        let mut host = self.world.non_send_mut::<ScriptHost>();
        let script = host.compile(source).unwrap();
        let mut budget = Budget::new(u64::MAX);
        host.call(&mut budget, script, "probe", (ctx, unit))
            .map_err(CallError::from_script)
    }

    /// The stable ids of `value`, a unit or a list of units.
    fn ids(value: Dynamic) -> Vec<StableId> {
        let units = match value.clone().try_cast::<Vec<Dynamic>>() {
            Some(units) => units,
            None if value.is_unit() => Vec::new(),
            None => vec![value],
        };
        units
            .into_iter()
            .map(|unit| unit.try_cast::<Unit>().unwrap().id)
            .collect()
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
    let entity = scene.entity(dead);
    scene.world.entity_mut(entity).insert(Dead);
    let team = |index| TeamSet::of(Team::new(index));

    // Before the first Vision stage each unit is seen by its team alone.
    assert_eq!(scene.seen_by(near), team(1));
    scene.world.run_schedule(SimUpdate);
    let teams = [seer, near, far, dead].map(|id| scene.seen_by(id));
    assert_eq!(
        teams,
        [team(0), team(1).with(Team::new(0)), team(1), team(2)]
    );

    // Seen through the view: `find` returns both enemies within 10 m, `find_visible` and
    // `nearest_visible` only the one team 0 sees, and `can_see` asks the other's teams.
    let read = |scene: &mut Scene, expression: &str, of| {
        let source = format!("fn probe(ctx, of) {{ {expression} }}");
        Scene::ids(scene.probe(&source, of).unwrap())
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
    let source = format!("fn probe(ctx, of) {{ {can_see} }}");
    let flags = scene.probe(&source, seer).unwrap().into_array().unwrap();
    let flags: Vec<bool> = flags
        .into_iter()
        .map(|flag| flag.as_bool().unwrap())
        .collect();
    assert_eq!(flags, [true, true, false]);

    // The seer steps 1 m: far's cell, now √6.5 m away, comes into sight in the next Vision stage.
    let entity = scene.entity(seer);
    *scene.world.get_mut::<Position>(entity).unwrap() = at(1, 0);
    assert_eq!(scene.seen_by(far), team(1));
    scene.world.run_schedule(SimUpdate);
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
    scene.world.run_schedule(SimUpdate);
    assert_eq!(scene.seen_by(sneak), team(1));
    let entity = scene.entity(ward);
    *scene.world.get_mut::<Position>(entity).unwrap() = at(-3, 0);
    scene.world.run_schedule(SimUpdate);
    assert_eq!(scene.seen_by(sneak), team(1).with(Team::new(0)));

    // What a team sees is state, restored with the rest.
    let mut per_type = Vec::new();
    scene.registry.hash_by_type(&scene.world, &mut per_type);
    let names: Vec<_> = per_type.iter().map(|TypeHash { name, .. }| *name).collect();
    assert!(names.contains(&"vision.seen_by") && names.contains(&"vision.sight"));
}

#[test]
fn set_bits_fills_a_run_within_one_word_and_across_words() {
    let mut words = [0; 3];
    set_bits(&mut words, 3..5);
    assert_eq!(words, [0b11000, 0, 0]);
    set_bits(&mut words, 63..64);
    assert_eq!(words, [0b11000 | 1 << 63, 0, 0]);
    // Bits 60 to 130: the top 4 of word 0, all of word 1, and the low 3 of word 2.
    let mut words = [0; 3];
    set_bits(&mut words, 60..131);
    assert_eq!(words, [0xF << 60, u64::MAX, 0b111]);
}
