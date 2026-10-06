//! The reference that every platform must reproduce bit for bit. Each section hashes a fixed
//! workload; a mismatch names the section, which points at the first divergence. The digests
//! change only when a release changes results on purpose.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

use bevy_ecs::entity::Entity;
use std::fmt::Write;
use std::num::NonZeroU32;

use bevy_ecs::component::Component;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::World;
use blake3::Hasher;
use campfire_common::SegmentSeed;
use campfire_log::internals::LogCheck;
use campfire_math::{Num, RngSource, RngStream, Vec3};
use campfire_sim::{
    EntityIndex, IdAllocator, SimComponent, SimRng, SimSet, SimTick, SimUpdate, StableId,
    StateRegistry, TickRate,
};
use serde::{Deserialize, Serialize};

/// 30 ticks a second: the rate these tests run at, which no game sets.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const CASES: usize = 4000;

/// `SplitMix64`: the same inputs on every platform, from a fixed seed. It is a copy of its own,
/// so that no other change can move the golden inputs.
#[derive(Debug)]
struct Inputs(u64);

impl Inputs {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Any raw value, a value within ±2⁴⁰ raw, or a small one, as the word chooses, so edges and
    /// common magnitudes both appear.
    const fn num(&mut self) -> Num {
        let word = self.next().cast_signed();
        Num::from_bits(match word.rem_euclid(3) {
            0 => word,
            1 => word % (1 << 40),
            _ => word % 4096,
        })
    }

    const fn within(&mut self, span: i64) -> Num {
        Num::from_bits(self.next().cast_signed() % span)
    }

    const fn vec3(&mut self, span: i64) -> Vec3 {
        Vec3::new(self.within(span), self.within(span), self.within(span))
    }
}

fn put_num(hasher: &mut Hasher, value: Num) {
    hasher.update(&value.to_bits().to_le_bytes());
}

fn put_option(hasher: &mut Hasher, value: Option<Num>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            put_num(hasher, value);
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn put_vec3(hasher: &mut Hasher, value: Option<Vec3>) {
    for part in value.map_or([None; 3], |v| [Some(v.x), Some(v.y), Some(v.z)]) {
        put_option(hasher, part);
    }
}

fn num_section(hasher: &mut Hasher) {
    let mut inputs = Inputs(1);
    for constant in [Num::PI, Num::TAU, Num::FRAC_PI_2] {
        put_num(hasher, constant);
    }
    for _ in 0..CASES {
        let (a, b) = (inputs.num(), inputs.num());
        let k = inputs.next().cast_signed() % 1000;
        for result in [
            a.checked_add(b),
            a.checked_sub(b),
            a.checked_mul(b),
            a.checked_div(b),
            a.checked_mul_int(k),
            a.checked_div_int(k),
            a.checked_sqrt(),
            a.to_string().parse().ok(),
        ] {
            put_option(hasher, result);
        }
        for integer in [a.floor(), a.ceil(), a.round()] {
            hasher.update(&integer.to_le_bytes());
        }
        hasher.update(a.to_string().as_bytes());
    }
}

fn trig_section(hasher: &mut Hasher) {
    let mut inputs = Inputs(2);
    let one = Num::ONE.to_bits();
    for _ in 0..CASES {
        for angle in [inputs.within(8 * one), inputs.num()] {
            let turn = angle.sin_cos();
            put_num(hasher, turn.sin);
            put_num(hasher, turn.cos);
        }
        put_num(hasher, inputs.num().atan2(inputs.num()));
        put_num(
            hasher,
            inputs.within(one / 64).atan2(inputs.within(1000 * one)),
        );
    }
}

fn vec3_section(hasher: &mut Hasher) {
    let mut inputs = Inputs(3);
    let span = 1000 << Num::FRAC_BITS;
    for _ in 0..CASES {
        let (a, b) = (inputs.vec3(span), inputs.vec3(span));
        put_option(hasher, a.checked_length());
        put_option(hasher, a.checked_distance(b));
        put_option(hasher, a.checked_dot(b));
        put_vec3(hasher, a.normalized());
        put_vec3(hasher, a.direction_to(b));
        put_vec3(hasher, a.checked_rotated_y(inputs.num().sin_cos()));
        hasher.update(&[u8::from(a.within(b, inputs.within(span)))]);
    }
}

fn rng_section(hasher: &mut Hasher) {
    let mut source = RngSource::new(SegmentSeed::new(*b"campfire golden test seed, 32 by"));
    for tick in 0..50 {
        source.begin_tick(tick);
        for entity in 0..8 {
            let mut rng = source.open(RngStream::new("golden"), entity);
            let draws = [
                rng.next_u64(),
                rng.below(1000),
                rng.below(u64::MAX),
                u64::from(rng.chance(Num::ONE / 3)),
                u64::from(rng.chance_ratio(1, 1_000_000)),
                rng.pick(7) as u64,
            ];
            for draw in draws {
                hasher.update(&draw.to_le_bytes());
            }
        }
    }
}

#[derive(Component, Debug, Serialize, Deserialize)]
struct Place(Vec3);

impl SimComponent for Place {
    const NAME: &'static str = "golden.place";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

#[derive(Component, Debug, Serialize, Deserialize)]
struct Life(Num);

impl SimComponent for Life {
    const NAME: &'static str = "golden.life";

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

fn state_section(hasher: &mut Hasher) {
    let mut inputs = Inputs(4);
    let mut world = World::new();
    world.init_resource::<EntityIndex>();
    world.init_resource::<IdAllocator>();
    let mut entities = Vec::new();
    for i in 0..300 {
        let id = world.resource_mut::<IdAllocator>().allocate();
        let mut entity = world.spawn((id, Place(inputs.vec3(1000 << Num::FRAC_BITS))));
        if i % 3 == 0 {
            entity.insert(Life(inputs.num()));
        }
        entities.push(entity.id());
    }
    for entity in entities.into_iter().step_by(7) {
        world.despawn(entity);
    }
    let mut state = StateRegistry::new();
    state.register_component::<Place>();
    state.register_component::<Life>();
    let mut per_type = Vec::new();
    let state_hash = state.hash_by_type(&world, &mut per_type);
    hasher.update(state_hash.as_bytes());
    let mut snapshot = Vec::new();
    state.snapshot(&world, &mut snapshot);
    hasher.update(&snapshot);
    let mut restored = World::new();
    state.restore(&snapshot, &mut restored).unwrap();
    assert_eq!(state.hash(&restored), state_hash);
    for type_hash in &per_type {
        hasher
            .update(type_hash.name.as_bytes())
            .update(type_hash.hash.as_bytes());
    }
}

/// Ticks in which one unit arrives, so units of every age move together.
const ARRIVAL_TICKS: u64 = 20;
const TICKS: usize = 100;

fn arrive(
    tick: Res<'_, SimTick>,
    mut ids: ResMut<'_, IdAllocator>,
    mut commands: Commands<'_, '_>,
) {
    if tick.start().get() < ARRIVAL_TICKS {
        commands.spawn((ids.allocate(), Place(Vec3::ZERO), Life(Num::ONE)));
    }
}

fn drift(rng: Res<'_, SimRng>, mut units: Query<'_, '_, (&StableId, &mut Place)>) {
    for (&id, mut place) in &mut units {
        let mut rng = rng.open(RngStream::new("golden.drift"), id);
        let mut axis = || Num::from_bits(rng.below(1 << 25).cast_signed()) - Num::ONE;
        place.0 += Vec3::new(axis(), axis(), axis());
    }
}

fn wear(rng: Res<'_, SimRng>, mut units: Query<'_, '_, (&StableId, &mut Life)>) {
    for (&id, mut life) in &mut units {
        if rng
            .open(RngStream::new("golden.wear"), id)
            .chance(Num::ONE / 3)
        {
            life.0 -= Num::ONE / 10;
        }
    }
}

/// The sim schedule over many ticks: the tick keys every draw, so a wrong tick number or a
/// missed tick start changes the digest.
fn tick_section(hasher: &mut Hasher) {
    let mut inputs = Inputs(5);
    let mut seed = [0; 32];
    for chunk in seed.as_chunks_mut::<8>().0 {
        *chunk = inputs.next().to_le_bytes();
    }
    let mut world = World::new();
    SimUpdate::prepare(&mut world, SegmentSeed::new(seed), RATE);
    let mut schedule = SimUpdate::schedule();
    schedule.add_systems((
        arrive.in_set(SimSet::Inputs),
        drift.in_set(SimSet::Act),
        wear.in_set(SimSet::Resolve),
    ));
    let mut state = StateRegistry::new();
    state.register_component::<Place>();
    state.register_component::<Life>();
    for _ in 0..TICKS {
        schedule.run(&mut world);
        hasher.update(state.hash(&world).as_bytes());
    }
}

/// A named workload and the digest every platform must reproduce.
#[derive(Debug)]
struct Section {
    name: &'static str,
    run: fn(&mut Hasher),
    digest: &'static str,
}

#[test]
fn golden_digests() {
    let _log = LogCheck::start();
    let sections = [
        Section {
            name: "num",
            run: num_section,
            digest: "db5de1ca295a6ef195f5a5f20d76c8ab0b111c0c90fe41a74e5c6d9522980c65",
        },
        Section {
            name: "trig",
            run: trig_section,
            digest: "d948c09fa43afd37f9529608a1af1748d23c3e8f530940daf3122ff5d3540176",
        },
        Section {
            name: "vec3",
            run: vec3_section,
            digest: "cbc5de4894e677c32439c9eedc47f1e2560a7b29a131419a87bc95c8c614fc2c",
        },
        Section {
            name: "rng",
            run: rng_section,
            digest: "3eb5a377b6a9d8b1dd05f4a11e437737655d55d42390aa05624e350f45fa586d",
        },
        Section {
            name: "state",
            run: state_section,
            digest: "443b14892444a7e1a629e15d6c2d53e7f34bab98c96f523b4c083a29f16bbae9",
        },
        Section {
            name: "tick",
            run: tick_section,
            digest: "e9893149cbba864817a3512da5641c052fcf4d79a177970f12e741634d403837",
        },
    ];
    let mut mismatches = String::new();
    for section in sections {
        let mut hasher = Hasher::new();
        (section.run)(&mut hasher);
        let actual = hasher.finalize().to_hex();
        if actual.as_str() != section.digest {
            write!(mismatches, "\n    {}: {actual}", section.name).unwrap();
        }
    }
    assert!(
        mismatches.is_empty(),
        "sections differ from the golden digests:{mismatches}"
    );
}
