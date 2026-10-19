use std::hint::black_box;

use campfire_combat::{Damage, DamageCause, DamageHandle};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_measure::{Allocations, Kind, Scale};
use campfire_script::{Budget, CallStart, Ctx, ListPlace, ScriptArgs, ScriptHost, ScriptId};
use campfire_sim::{SimTick, StableId};
use campfire_units::Units;
use campfire_values::{DamageKind, UnitType};
use criterion::Criterion;

use crate::fixed_session::fixed_match::FixedMatch;
use crate::moba_3v3::Moba3v3;

/// A case of the allocation benches: a query, a creep's think, or the mode's weigh of a damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    Find,
    Think,
    Weigh,
}

impl Case {
    const ALL: [Self; 3] = [Self::Find, Self::Think, Self::Weigh];

    const fn name(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::Think => "on_think",
            Self::Weigh => "calc_damage",
        }
    }
}

/// The 3v3 at its start with its creeps crowded, its host holding the scripts of the cases, and
/// the unit each case calls for: the melee creep of the lowest stable id, whose AI `on_think`
/// runs.
#[derive(Debug)]
struct CallScene {
    fixed: FixedMatch,
    unit: StableId,
    find: ScriptId,
    ai: ScriptId,
    mode: ScriptId,
}

impl CallScene {
    fn new() -> Self {
        let moba = Moba3v3::load();
        let source = |path: &str| {
            let scripts = moba.packages().scripts();
            let script = scripts
                .iter()
                .find(|script| script.path.to_string() == path);
            script.expect("a script of the 3v3").source.to_owned()
        };
        let (ai, mode) = (source("scripts/creep_ai.rhai"), source("scripts/mode.rhai"));
        let mut fixed = moba.start();
        let world = fixed.game_mut();
        Moba3v3::crowd_creeps(world);
        let creep = Units::type_named(world, "melee_creep").expect("the 3v3's melee creep");
        let unit = world
            .query::<(&StableId, &UnitType)>()
            .iter(world)
            .filter_map(|(&id, &unit_type)| (unit_type == creep).then_some(id))
            .min()
            .expect("the crowd holds creeps");
        let mut host = world.non_send_mut::<ScriptHost>();
        let find = r#"fn probe(ctx, of) { ctx.find(of, of.position, 8, "enemies") }"#;
        Self {
            unit,
            find: host.compile(find).expect("the probe compiles"),
            ai: host.compile(&ai).expect("the creep AI compiles"),
            mode: host.compile(&mode).expect("the mode's script compiles"),
            fixed,
        }
    }

    /// Makes `case`'s call once, at `now`, through `ctx`, from `budget`.
    fn call(&mut self, case: Case, ctx: &Ctx, now: Tick, budget: &mut Budget) {
        let world = self.fixed.game_mut();
        let unit = self.unit;
        let handle = ctx.view().unit(unit).expect("a unit of the 3v3");
        let start = match case {
            Case::Find | Case::Think => CallStart::think(unit, now),
            // The 3v3 lists its own script alone.
            Case::Weigh => CallStart::pure(ListPlace::FIRST, now),
        };
        ctx.frame().begin_alone(world, start).unwrap();
        let mut host = world.non_send_mut::<ScriptHost>();
        let called = match case {
            Case::Find => {
                let args = (ctx.clone(), handle).into_engine();
                host.call(budget, self.find, "probe", args)
            }
            Case::Think => {
                let args = (ctx.clone(), handle).into_engine();
                host.call(budget, self.ai, "on_think", args)
            }
            Case::Weigh => {
                let damage = Damage {
                    source: None,
                    target: unit,
                    amount: Num::from_int(40).unwrap(),
                    kind: DamageKind::new(0),
                    cause: DamageCause::Effect,
                    action: None,
                    depth: 0,
                    hit: None,
                };
                let damage = DamageHandle::new(damage, ctx.view().clone());
                let args = (ctx.clone(), damage).into_engine();
                host.call(budget, self.mode, "calc_damage", args)
            }
        };
        drop(black_box(called.expect("the case's call runs")));
    }
}

/// The allocations of a call of each of the 3v3's hot scripts, for one creep of the crowded 3v3
/// at its start: `find`, a query of the enemies near the unit; `on_think`, a creep's AI as it
/// thinks; and `calc_damage`, the mode's weigh of a damage the unit takes, as the damage pass
/// calls it, each with its call started, as the architecture review's decision D30 asks.
pub(crate) fn allocations(c: &mut Criterion<Allocations>) {
    let mut scene = None;
    let mut group = Kind::Integration.group(c, "script_allocations", Scale::Sample);
    for case in Case::ALL {
        group.bench_function(case.name(), |b| {
            let scene = scene.get_or_insert_with(CallScene::new);
            let world = scene.fixed.game_mut();
            let ctx = world.non_send::<Ctx>().clone();
            ctx.view().read(world);
            let now = world.resource::<SimTick>().start();
            let mut budget = Budget::new(u64::MAX);
            b.iter(|| scene.call(case, &ctx, now, &mut budget));
        });
    }
    group.finish();
}
