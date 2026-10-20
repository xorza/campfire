use std::cell::LazyCell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::world::World;
use campfire_capabilities::Match;
use campfire_measure::{Kind, Scale};
use campfire_protocol::SessionLog;
use campfire_runner::SessionWorld;
use campfire_sim::internals::StageClock;
use campfire_sim::{SimSystems, SimWorld, StateDelta};
use campfire_stats::Pools;
use campfire_units::Life;
use criterion::{Criterion, Throughput};

use crate::fixed_session::fixed_match::FixedMatch;
use crate::moba_3v3::Moba3v3;
use crate::scripted_mode::ScriptedMode;

/// The ticks of a measured 3v3 match: 5 minutes at 20 Hz, the pick, then a wave every 30 s from
/// tick 2399, which fight their lanes' towers.
const TICKS: u64 = 6000;
/// The ticks of a battle case: 3 s at the 3v3's 20 Hz.
const BATTLE_TICKS: u64 = 60;

/// The server's tick of the MOBA 3v3, whole and by stage, from one load of its packages.
pub(crate) fn server(c: &mut Criterion) {
    let moba: LazyCell<ScriptedMode<Moba3v3>> = LazyCell::new(Moba3v3::load);
    server_tick(c, &moba);
    server_stage(c, &moba);
    battle(c, &moba);
    session_log(c, &moba);
}

/// A tick of the MOBA 3v3 as its packages hold it: its first tick, `first_3v3`, which
/// builds the sim schedule and labels the pathing grid around the map's static bodies; the worst
/// of the other ticks of a match of `TICKS` ticks, with no checkpoint and with the main thread's
/// part of a delta every tick, the copy of the state that changed, as a server sends one whenever
/// its checkpoint thread is free; and
/// the mean tick of such a match, `mean_3v3`, a whole match each iteration, its throughput the
/// ticks. A rollback re-simulates whole ticks, so it costs its depth times these.
fn server_tick(c: &mut Criterion, moba: &LazyCell<ScriptedMode<Moba3v3>>) {
    let mut group = Kind::Integration.group(c, "server_tick", Scale::Frame);
    group.bench_function("first_3v3", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| timed_tick(moba.start().game_mut()))
                .sum()
        });
    });
    group.bench_function("worst_3v3", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| MatchCost::of_match(&mut moba.start(), timed_tick).worst)
                .sum()
        });
    });
    group.bench_function("worst_3v3_checkpointed", |b| {
        let mut delta = StateDelta::default();
        b.iter_custom(|matches| {
            let mut worst_sum = Duration::ZERO;
            for _ in 0..matches {
                let mut fixed = moba.start();
                fixed.game_mut().track(&mut delta);
                let cost = MatchCost::of_match(&mut fixed, |game| {
                    let start = Instant::now();
                    game.run_tick_alone();
                    game.changes(&mut delta);
                    let spent = start.elapsed();
                    black_box(&delta);
                    spent
                });
                worst_sum += cost.worst;
            }
            worst_sum
        });
    });
    group.throughput(Throughput::Elements(TICKS));
    group.bench_function("mean_3v3", |b| {
        b.iter_custom(|matches| {
            (0..matches)
                .map(|_| MatchCost::of_match(&mut moba.start(), timed_tick).total)
                .sum()
        });
    });
    group.finish();
}

/// Each stage of a tick of the MOBA 3v3, as `StageClock`'s probes time it in the match: its
/// worst in a match of `TICKS` ticks, `worst_3v3_<stage>`, and its mean tick in such a match,
/// `mean_3v3_<stage>`, a whole match each iteration, its throughput the ticks. The stages'
/// means add up to `integration/server_tick/mean_3v3` less the parts of a tick that run outside
/// them: the session log's, the tick's start and end, and `SimEdgeSystems::Start`.
fn server_stage(c: &mut Criterion, moba: &LazyCell<ScriptedMode<Moba3v3>>) {
    let id = |statistic: &str, stage: SimSystems| {
        format!("{statistic}_3v3_{}", format!("{stage:?}").to_lowercase())
    };
    let mut group = Kind::Integration.group(c, "server_stage", Scale::Frame);
    let stage_match =
        |stage: SimSystems| MatchCost::of_match(&mut clocked(moba), |game| stage_tick(game, stage));
    for stage in SimSystems::ALL {
        group.bench_function(id("worst", stage), |b| {
            b.iter_custom(|matches| {
                (0..matches)
                    .map(|_| stage_match(stage).worst_of_all())
                    .sum()
            });
        });
    }
    group.throughput(Throughput::Elements(TICKS));
    for stage in SimSystems::ALL {
        group.bench_function(id("mean", stage), |b| {
            b.iter_custom(|matches| (0..matches).map(|_| stage_match(stage).total).sum());
        });
    }
    group.finish();
}

/// Each stage of a tick of a battle: the 3v3 at its start with `KernelScene::UNITS` of its creeps
/// more, both teams mixed in the crowded scene's square, which fight from their spawn. Its case
/// `crowded_<stage>` is the stage's mean tick over `BATTLE_TICKS` ticks of a new battle each
/// iteration, its throughput the ticks: the creeps' targeting, strikes, bolts and damage at the
/// scale of a large battle, where a 3v3 match's few creeps leave them small.
fn battle(c: &mut Criterion, moba: &LazyCell<ScriptedMode<Moba3v3>>) {
    let mut group = Kind::Integration.group(c, "battle", Scale::Frame);
    group.throughput(Throughput::Elements(BATTLE_TICKS));
    for stage in SimSystems::ALL {
        let id = format!("crowded_{}", format!("{stage:?}").to_lowercase());
        group.bench_function(id, |b| {
            b.iter_custom(|battles| {
                (0..battles)
                    .map(|_| {
                        let mut fixed = clocked(moba);
                        Moba3v3::crowd_creeps(fixed.game_mut());
                        let spent = (0..BATTLE_TICKS)
                            .map(|_| stage_tick(fixed.game_mut(), stage))
                            .sum::<Duration>();
                        assert!(is_hurt(fixed.game_mut()), "the creeps fight");
                        spent
                    })
                    .sum()
            });
        });
    }
    group.finish();
}

/// A match of the MOBA 3v3 that its players' scripted orders played for `TICKS` ticks, and its
/// session log's file.
#[derive(Debug)]
struct PlayedLog {
    fixed: FixedMatch,
    file: Vec<u8>,
}

impl PlayedLog {
    fn new(moba: &ScriptedMode<Moba3v3>) -> Self {
        let mut fixed = moba.start();
        for tick in 0..TICKS {
            moba.play_tick(&mut fixed, tick);
        }
        let mut file = Vec::new();
        fixed.game().session().log().encode(&mut file);
        Self { fixed, file }
    }
}

/// The session log of a match of the MOBA 3v3 that its players' scripted orders play for `TICKS`
/// ticks, as a server writes it at the session's end and a verifier reads it: `encode_3v3`, its
/// file written; and `decode_3v3`, that file read back, which records each entry again and so
/// checks each chain link and signature.
fn session_log(c: &mut Criterion, moba: &LazyCell<ScriptedMode<Moba3v3>>) {
    let mut played = None;
    let mut out = Vec::new();
    let mut group = Kind::Integration.group(c, "session_log", Scale::Sample);
    group.bench_function("encode_3v3", |b| {
        let PlayedLog { fixed, .. } = played.get_or_insert_with(|| PlayedLog::new(moba));
        b.iter(|| {
            out.clear();
            fixed.game().session().log().encode(&mut out);
            black_box(&out);
        });
    });
    group.bench_function("decode_3v3", |b| {
        let PlayedLog { file, .. } = played.get_or_insert_with(|| PlayedLog::new(moba));
        b.iter(|| black_box(SessionLog::decode(black_box(file)).expect("the log reads back")));
    });
    group.finish();
}

/// Whether a unit of `world` holds its life or a pool below its maximum, as one that took damage
/// does.
fn is_hurt(world: &mut World) -> bool {
    let mut lives = world.query::<&Life>();
    let mut pools = world.query::<&Pools>();
    lives.iter(world).any(|life| life.current() < life.max())
        || pools.iter(world).any(|pools| {
            pools
                .ids()
                .any(|pool| pools.current(pool) < pools.max(pool))
        })
}

/// What a match of `TICKS` ticks cost, by the time `tick` gives for each of its ticks: in all,
/// its first tick, and the worst of the others.
#[derive(Debug, Clone, Copy)]
struct MatchCost {
    total: Duration,
    first: Duration,
    worst: Duration,
}

impl MatchCost {
    /// Plays `fixed` for `TICKS` ticks, each by `tick`, which runs it and gives its time.
    fn of_match(fixed: &mut FixedMatch, mut tick: impl FnMut(&mut Match) -> Duration) -> Self {
        Self::of(|| tick(fixed.game_mut()))
    }

    /// The cost of `TICKS` ticks, each the time `tick` gives.
    fn of(mut tick: impl FnMut() -> Duration) -> Self {
        let first = tick();
        let mut cost = Self {
            total: first,
            first,
            worst: Duration::ZERO,
        };
        for _ in 1..TICKS {
            let spent = tick();
            cost.total += spent;
            cost.worst = cost.worst.max(spent);
        }
        cost
    }

    /// The worst of all its ticks, the first among them.
    fn worst_of_all(self) -> Duration {
        self.first.max(self.worst)
    }
}

/// A new match of `moba`, with `StageClock`'s probes in its schedule.
fn clocked(moba: &ScriptedMode<Moba3v3>) -> FixedMatch {
    let mut fixed = moba.start();
    StageClock::install(fixed.game_mut());
    fixed
}

/// Runs a tick of `game`, whose schedule holds `StageClock`'s probes, and gives `stage`'s time
/// in it.
fn stage_tick(game: &mut Match, stage: SimSystems) -> Duration {
    game.run_tick_alone();
    let spent = game.resource::<StageClock>().stage(stage);
    black_box(&game);
    spent
}

fn timed_tick(game: &mut Match) -> Duration {
    let start = Instant::now();
    game.run_tick_alone();
    let spent = start.elapsed();
    black_box(&game);
    spent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_cost_splits_the_first_tick_from_the_worst_of_the_rest() {
        // Tick 0 takes 9 µs and tick n after it n % 7 µs: the 5,999 ticks after the first are
        // 857 whole rounds of 1 to 6 and 0, 21 µs each, so 17,997 µs, and 18,006 µs with the
        // first. The worst after the first is 6 µs; of all, the first's 9 µs.
        let mut n = 0;
        let cost = MatchCost::of(|| {
            let micros = if n == 0 { 9 } else { n % 7 };
            n += 1;
            Duration::from_micros(micros)
        });
        assert_eq!(n, TICKS);
        assert_eq!(cost.first, Duration::from_micros(9));
        assert_eq!(cost.worst, Duration::from_micros(6));
        assert_eq!(cost.total, Duration::from_micros(18_006));
        assert_eq!(cost.worst_of_all(), Duration::from_micros(9));
    }
}
