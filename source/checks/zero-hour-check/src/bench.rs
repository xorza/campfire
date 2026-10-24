//! The benches of the Zero Hour plays, on the import of the player's own copy that a run of the
//! check wrote to `~/zh-package`; with no import there, they register no case, so no fresh clone
//! and no CI runs them.

use std::env;
use std::num::NonZeroU32;
use std::time::Duration;

use campfire_common::MapName;
use campfire_harness::FixedSession;
use campfire_import::ZeroHour;
use campfire_measure::{Kind, RunCost, Scale};
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;
use campfire_sim::SimSystems;
use campfire_sim::internals::StageClock;
use criterion::Criterion;

use crate::attack_play::AttackPlay;
use crate::base_play::BasePlay;
use crate::command_line::CommandLine;
use crate::match_play::{MatchPlay, PlayRecord, PlayTools};
use crate::pins::{HZ, MAP};

/// The ticks of a run: 200 s at the plays' 30 ticks a second, as long as the 3v3's server tick
/// runs, through each play's building and its first fights.
const TICKS: u64 = 6000;

/// A play of the check whose ticks a case times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Play {
    /// China's and GLA's bases from their base starts, the mode's attacker driving each.
    Bases,
    /// China's attacker and GLA's builder from their standard starts.
    Attack,
}

impl Play {
    const ALL: [Self; 2] = [Self::Bases, Self::Attack];

    const fn name(self) -> &'static str {
        match self {
            Self::Bases => "bases",
            Self::Attack => "attack",
        }
    }

    /// The session of the play, of `packages`.
    fn session(self, packages: ModePackages) -> FixedSession {
        let rate = u32::try_from(HZ).ok().and_then(NonZeroU32::new);
        let rate = rate.expect("a rate");
        let session = match self {
            Self::Bases => BasePlay::session(packages, rate),
            Self::Attack => AttackPlay::session(packages, rate),
        };
        session.expect("the play's session plans")
    }

    /// The cost of a run of `TICKS` ticks of a new match of the play's `session` from its start,
    /// each tick's time the one `tick` gives as it runs the tick; the stages' probes installed
    /// with `clocked`.
    fn run(
        self,
        session: &FixedSession,
        clocked: Clocked,
        mut tick: impl FnMut(&mut MatchPlay<'_>) -> Duration,
    ) -> RunCost {
        let tools = PlayTools {
            probe: None,
            record: PlayRecord::None,
        };
        let steps = |play: &mut MatchPlay<'_>| {
            if clocked == Clocked::Stages {
                StageClock::install(play.game_mut());
            }
            RunCost::of(TICKS, || tick(play))
        };
        let mut failures = Vec::new();
        let cost = match self {
            Self::Bases => BasePlay::with_play(session, tools, &mut failures, steps),
            Self::Attack => AttackPlay::with_play(session, tools, steps),
        };
        assert!(failures.is_empty(), "the bases stand as the check has them");
        cost.expect("the play runs")
    }
}

/// Whether a run installs the probes that time each stage of a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clocked {
    Stages,
    Plain,
}

/// Runs each bench of the crate whose id criterion's filter takes: with the import of
/// `~/zh-package`, the sim's tick of each play, `integration/zero_hour_tick/<case>_<play>`; its
/// bots' think, `integration/zero_hour_bots/<case>_<play>`, as a server's bot driver runs it
/// before the tick; and each stage of the bases play's tick,
/// `integration/zero_hour_stage/<case>_bases_<stage>`, as `StageClock`'s probes time it; each
/// case its worst over a run of `TICKS` ticks, the first among them, or its mean.
pub fn run(c: &mut Criterion) {
    let map = MapName::parse(MAP).expect("a map's name");
    let game = env::home_dir()
        .map(|home| PackageDir::new(home.join(CommandLine::HOME_PACKAGES).join(ZeroHour::GAME)));
    let sessions = Play::ALL.map(|play| {
        let packages = game.as_ref().map(|game| ModePackages::at(game, Some(&map)));
        packages
            .and_then(Result::ok)
            .map(|packages| play.session(packages))
    });
    let [Some(bases), Some(attack)] = sessions else {
        skipped();
        return;
    };
    let played = [(Play::Bases, &bases), (Play::Attack, &attack)];
    let mut group = Kind::Integration.group(c, "zero_hour_tick", Scale::Frame);
    for (play, session) in played {
        RunCost::cases(&mut group, play.name(), TICKS, || {
            play.run(session, Clocked::Plain, |play| play.timed_tick().tick)
        });
    }
    group.finish();
    let mut group = Kind::Integration.group(c, "zero_hour_bots", Scale::Frame);
    for (play, session) in played {
        RunCost::cases(&mut group, play.name(), TICKS, || {
            play.run(session, Clocked::Plain, |play| play.timed_tick().think)
        });
    }
    group.finish();
    let mut group = Kind::Integration.group(c, "zero_hour_stage", Scale::Frame);
    for stage in SimSystems::ALL {
        let name = format!("bases_{}", format!("{stage:?}").to_lowercase());
        RunCost::cases(&mut group, &name, TICKS, || {
            Play::Bases.run(&bases, Clocked::Stages, |play| {
                play.timed_tick();
                play.game().resource::<StageClock>().stage(stage)
            })
        });
    }
    group.finish();
}

/// Tells the bench's runner that its cases skip, and why.
#[expect(
    clippy::print_stderr,
    reason = "a bench of no case tells its runner why, where criterion prints"
)]
fn skipped() {
    eprintln!(
        "integration/zero_hour_*: no import of Zero Hour in ~/{}, so no case runs; \
         `cargo drive run zero-hour` writes one",
        CommandLine::HOME_PACKAGES
    );
}
