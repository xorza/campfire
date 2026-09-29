//! The reference that every platform must reproduce bit for bit. Each section hashes a fixed
//! workload; a mismatch names the section, which points at the first divergence. The digests
//! change only when a release changes results on purpose.

use std::fmt::Write;

use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use blake3::Hasher;
use campfire_math::{Num, RngSource, SegmentSeed, Vec3};
use campfire_sim::{EntityIndex, IdAllocator, SimComponent, StateHasher};
use serde::Serialize;

const CASES: usize = 4000;

/// `SplitMix64`: the same inputs on every platform, from a fixed seed. It is a copy of its own,
/// so that no other change can move the golden inputs.
#[derive(Debug)]
struct Inputs(u64);

impl Inputs {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Any raw value, a value within ±2⁴⁰ raw, or a small one, as the word chooses, so edges and
    /// common magnitudes both appear.
    fn num(&mut self) -> Num {
        let word = self.next().cast_signed();
        Num::from_bits(match word.rem_euclid(3) {
            0 => word,
            1 => word % (1 << 40),
            _ => word % 4096,
        })
    }

    fn within(&mut self, span: i64) -> Num {
        Num::from_bits(self.next().cast_signed() % span)
    }

    fn vec3(&mut self, span: i64) -> Vec3 {
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
            let mut rng = source.open("golden", entity);
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

#[derive(Component, Debug, Serialize)]
struct Place(Vec3);

impl SimComponent for Place {
    const NAME: &'static str = "golden.place";
}

#[derive(Component, Debug, Serialize)]
struct Life(Num);

impl SimComponent for Life {
    const NAME: &'static str = "golden.life";
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
    let mut state = StateHasher::new();
    state.register_component::<Place>();
    state.register_component::<Life>();
    let mut per_type = Vec::new();
    hasher.update(state.hash_by_type(&world, &mut per_type).as_bytes());
    for type_hash in &per_type {
        hasher
            .update(type_hash.name.as_bytes())
            .update(&type_hash.hash);
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
            digest: "96485cc954c708b828f2089562eafc08ecb62ff6da1569a0616e819fad9ee190",
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
