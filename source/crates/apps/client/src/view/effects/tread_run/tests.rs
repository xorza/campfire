use campfire_client_files::{EmissionVelocity, ParticleSystem, SpeedScale, Spread};

use super::*;
use crate::view::effects::particle_run::samples::{book, name, system};

fn level(_: f32, _: f32) -> f32 {
    0.0
}

#[test]
fn a_tread_runs_its_systems_scaled_by_its_speed_and_stops_them_as_it_stands() {
    // A system of bursts of 4, each particle at (1, 1, 1) a step; dust that runs past 0.5 m a
    // second, scaled across by 0.125 + speed/32, up by 0.25 + speed/16 and in count by
    // 0.5 + speed/32, each at most 1.
    let book = book(
        100,
        [(
            "dust",
            ParticleSystem {
                burst_count: Spread::of(4.0),
                velocity: EmissionVelocity::Ortho {
                    x: Spread::of(1.0),
                    y: Spread::of(1.0),
                    z: Spread::of(1.0),
                },
                ..system()
            },
        )],
    );
    let scale = |base, per_speed| SpeedScale {
        base,
        per_speed,
        most: 1.0,
    };
    let mut treads = TreadRun::new(TreadDust {
        moving: 0.5,
        across: scale(0.125, 1.0 / 32.0),
        rise: scale(0.25, 1.0 / 16.0),
        count: scale(0.5, 1.0 / 32.0),
    });
    let mut run = ParticleRun::new(1);
    let dust = book.id(&name("dust")).unwrap();
    let key = run.start(&book, dust, Affine3A::IDENTITY);
    let root = Entity::from_raw_u32(1).unwrap();
    treads.add(root, [key]);
    let at = Affine3A::from_translation(Vec3::new(5.0, 0.0, 0.0));
    let drive = |treads: &mut TreadRun, run: &mut ParticleRun, velocity: Vec3| {
        treads.drive(run, |_| Some(Driven { at, velocity }));
        run.step(&book, level);
        run.particles()
            .map(|particle| particle.position)
            .collect::<Vec<_>>()
    };
    // Rising at 3 m a second, it stands on the ground: no dust.
    assert_eq!(drive(&mut treads, &mut run, Vec3::Y * 3.0), []);
    // At 16 m a second along z: 4 particles from (5, 0, 0), each at (0.625, 1, 0.625) a step,
    // 0.125 + 0.5 across, 0.25 + 1 up, at most 1, and 0.5 + 0.5 of the count.
    let driving = drive(&mut treads, &mut run, Vec3::Z * 16.0);
    assert_eq!(driving, [Vec3::new(5.625, 1.0, 0.625); 4]);
    // At 8 m a second along x, 0.75 of the count: 3 more.
    assert_eq!(drive(&mut treads, &mut run, Vec3::X * 8.0).len(), 7);
    // Its drawing gone, its systems go, and it throws no more.
    treads.drive(&mut run, |_| None);
    run.step(&book, level);
    assert_eq!(run.particles().count(), 7);
    assert!(treads.treads.is_empty());
}
