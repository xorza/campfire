use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

use campfire_combat::{Damage, DamageCause, DamageHandle};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_script::{Budget, CallStart, Ctx, ListPlace, ScriptArgs, ScriptHost, ScriptId};
use campfire_sim::{SimTick, StableId};
use campfire_units::Units;
use campfire_values::{DamageKind, UnitType};
use criterion::measurement::{Measurement, ValueFormatter};
use criterion::{Criterion, Throughput};

use crate::fixed_session::fixed_match::FixedMatch;
use crate::moba_3v3::Moba3v3;

/// The allocations the process made, as `CountingAllocator` counts them: the outside world's
/// allocator interface keeps its state in a static.
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// The global allocator of the allocation benches: the system's, which counts each allocation,
/// a growth among them.
#[derive(Debug)]
pub struct CountingAllocator;

// SAFETY: each call passes its arguments to the system's allocator unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Criterion's measure of the allocations a sample makes, as `CountingAllocator` counts them.
#[derive(Debug)]
pub struct Allocations;

impl Measurement for Allocations {
    type Intermediate = u64;
    type Value = u64;

    fn start(&self) -> u64 {
        ALLOCATIONS.load(Ordering::SeqCst)
    }

    fn end(&self, start: u64) -> u64 {
        ALLOCATIONS.load(Ordering::SeqCst) - start
    }

    fn add(&self, a: &u64, b: &u64) -> u64 {
        a + b
    }

    fn zero(&self) -> u64 {
        0
    }

    fn to_f64(&self, value: &u64) -> f64 {
        *value as f64
    }

    fn formatter(&self) -> &dyn ValueFormatter {
        &AllocationsFormatter
    }
}

/// The unit of `Allocations`, unscaled.
#[derive(Debug)]
struct AllocationsFormatter;

impl ValueFormatter for AllocationsFormatter {
    fn scale_values(&self, _: f64, _: &mut [f64]) -> &'static str {
        "allocations"
    }

    fn scale_throughputs(&self, _: f64, _: &Throughput, _: &mut [f64]) -> &'static str {
        "allocations"
    }

    fn scale_for_machines(&self, _: &mut [f64]) -> &'static str {
        "allocations"
    }
}

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
    let mut group = c.benchmark_group("integration/script_allocations");
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
