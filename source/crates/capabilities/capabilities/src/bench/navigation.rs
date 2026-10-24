use std::hint::black_box;
use std::time::{Duration, Instant};

use campfire_declarations::Role;
use campfire_geometry::internals::KernelScene;
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, RunCost, Scale};
use campfire_navigation::{Gait, Navigation, UnitNavigationData, Walker};
use campfire_sim::{Capability, SimWorld};
use campfire_units::{BodyForm, BodyShape, Layer};
use campfire_values::Team;
use criterion::{Criterion, Throughput};

use crate::fixtures::navigation::NavigationFixtures;
use crate::release::r#match::Match;

/// The radius of each unit, in meters, as a soldier's or a light vehicle's.
const RADIUS: Num = Num::HALF;

/// A crowd that navigation moves, of `KernelScene::UNITS` units of one team on half-meter cells,
/// each of a half meter, a quarter meter a tick, so a tick is navigation's alone: its route asks,
/// its steering with its steps aside and its yields, and its collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scene {
    /// Walkers 1.5 m apart, 40 by 25, each to the point 80 m east of it, as an army that crosses
    /// open ground.
    Open,
    /// Walkers 1.5 m apart, 25 by 20, each to the point 90 m east of it, through as many idle
    /// units of their team 1.2 m apart, 25 by 20, that stand between, as an army that crosses its
    /// base.
    Crowd,
    /// Units that drive, two abreast 1.2 m apart, in 10 lanes 3 m wide and 60 m long between walls,
    /// from x = −40 to 20, open at both ends: in each, 40 idle units at the lane's east end, and
    /// 60 walkers west of the lane, each to the point 100 m east of it, past the lane, as dozers
    /// sent past a column of idle ones (04-capabilities/navigation.md, A friend yields through the
    /// friends that yield).
    Corridor,
}

const _: () = assert!(40 * 25 == KernelScene::UNITS, "the open scene's units");
const _: () = assert!(2 * 25 * 20 == KernelScene::UNITS, "the crowd scene's units");
const _: () = assert!(
    10 * (40 + 60) == KernelScene::UNITS,
    "the corridor scene's units"
);

impl Scene {
    const ALL: [Self; 3] = [Self::Open, Self::Crowd, Self::Corridor];

    /// The ticks of a run of the scene: past its walkers' arrivals, as the walks of 80 m and more
    /// take 320 ticks at the least, and the crowd and the column hold the walkers back.
    const fn ticks(self) -> u64 {
        match self {
            Self::Open => 320,
            Self::Crowd => 1280,
            Self::Corridor => 1440,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Crowd => "crowd",
            Self::Corridor => "corridor",
        }
    }

    /// A match of the scene's units, its walkers sent.
    fn build(self) -> Match {
        let mut game = Match::builder(&[Capability::Navigation])
            .role(Role::Rules)
            .build();
        let walker = Walker {
            layer: Layer::FIRST,
            radius: RADIUS,
        };
        Navigation::load_open_pathing(&mut game, Num::HALF, [-80, -80], [80, 80], [walker]);
        let unit_type = Navigation::walker_type(&mut game);
        let gait = match self {
            Self::Open | Self::Crowd => Gait::Step,
            Self::Corridor => Gait::Drive,
        };
        let navigation = UnitNavigationData::new(gait, Num::ONE).expect("a weight above zero");
        Navigation::load_unit_navigation(&mut game, unit_type, navigation);
        let at = |x: Num, z: Num| Vec3::new(x, Num::ZERO, z);
        let unit = |game: &mut Match, position: Vec3, to: Option<Vec3>| {
            Navigation::walker_at(game, position)
                .team(Team::new(0))
                .circle(RADIUS)
                .walks(Num::QUARTER, to)
                .place(game);
        };
        let apart = |count: i64, step: Num| step * Num::int(count);
        let wide = Num::int(3) / 2;
        let near = Num::int(6) / 5;
        match self {
            Self::Open => {
                for nth in 0..40 * 25 {
                    let from = at(apart(nth % 40, wide) - Num::int(70), apart(nth / 40, wide));
                    unit(&mut game, from, Some(from + Vec3::from((80, 0, 0))));
                }
            }
            Self::Crowd => {
                for nth in 0..25 * 20 {
                    let (column, row) = (nth % 25, nth / 25);
                    let idle = at(
                        apart(column, near) - Num::int(15),
                        apart(row, near) - Num::int(12),
                    );
                    unit(&mut game, idle, None);
                    let from = at(
                        apart(column, wide) - Num::int(75),
                        apart(row, wide) - Num::int(15),
                    );
                    unit(&mut game, from, Some(from + Vec3::from((90, 0, 0))));
                }
            }
            Self::Corridor => {
                let wall = BodyForm::new(
                    BodyShape::box_sized([Num::int(60), Num::ONE]).expect("a wall's sides"),
                    Layer::FIRST,
                );
                for nth in 0..=10 {
                    let z = Num::int(4 * nth - 20);
                    Navigation::walker_at(&mut game, at(Num::int(-10), z))
                        .body(wall)
                        .place(&mut game);
                }
                let abreast = Num::int(3) / 5;
                for lane in 0..10 {
                    let middle = Num::int(4 * lane - 18);
                    for nth in 0..40 {
                        let z = middle + if nth % 2 == 0 { -abreast } else { abreast };
                        unit(&mut game, at(apart(nth / 2, near) - Num::int(5), z), None);
                    }
                    for nth in 0..60 {
                        let z = middle + if nth % 2 == 0 { -abreast } else { abreast };
                        let from = at(apart(nth / 2, near) - Num::int(77), z);
                        unit(&mut game, from, Some(from + Vec3::from((100, 0, 0))));
                    }
                }
            }
        }
        game
    }

    /// The cost of a run of a new match of the scene.
    fn run(self) -> RunCost {
        let mut game = self.build();
        RunCost::of(self.ticks(), || {
            let start = Instant::now();
            game.step();
            let spent = start.elapsed();
            black_box(&game);
            spent
        })
    }
}

/// A run of each scene of navigation's crowds from its first tick, a new match each iteration: its
/// worst tick, `worst_<scene>`, the first among them, as the walkers' first routes ask in it; and
/// its mean tick, `mean_<scene>`, its throughput the ticks.
pub(super) fn navigation(c: &mut Criterion) {
    let mut group = Kind::Integration.group(c, "navigation", Scale::Frame);
    for scene in Scene::ALL {
        group.bench_function(format!("worst_{}", scene.name()), |bench| {
            bench.iter_custom(|runs| (0..runs).map(|_| scene.run().worst_of_all()).sum());
        });
    }
    for scene in Scene::ALL {
        group.throughput(Throughput::Elements(scene.ticks()));
        group.bench_function(format!("mean_{}", scene.name()), |bench| {
            bench.iter_custom(|runs| (0..runs).map(|_| scene.run().total).sum::<Duration>());
        });
    }
    group.finish();
}
