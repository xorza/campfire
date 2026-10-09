use std::env;
use std::fmt::{self, Write as _};
use std::path::PathBuf;

use campfire_capabilities::{
    Dead, Deaths, Owner, PlayerResources, Pools, ResourceId, ScriptFailures, Team,
};
use campfire_common::{PlayerSlot, StateHash, Tick};
use campfire_log::ErrorReport;
use campfire_package::ModePackages;
use campfire_sim::{EntityIndex, Position, SimTick, StableId, StateRegistry};
use campfire_store::{DurableFile, InputFile};

use crate::runner::Runner;

/// The pinned record of a whole match, one line a tick: the state hash, which changes with the
/// state's layout, and a digest of the match's behaviour, which does not. A change of layout
/// alone moves only the first column; a change of behaviour moves the second, and names itself.
/// `CAMPFIRE_BLESS=1` writes the record again.
#[derive(Debug)]
pub struct Golden {
    resources: Vec<ResourceId>,
    players: u32,
    ticks: Vec<GoldenTick>,
    /// The bytes of one tick's behaviour, kept between ticks.
    scratch: Vec<u8>,
}

/// One tick of a `Golden`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GoldenTick {
    state: StateHash,
    behaviour: StateHash,
}

/// The digits of each hash a line keeps: 64 bits tell a changed tick apart.
const DIGITS: usize = 16;

impl Golden {
    /// A record of a match of `packages` for `players` players.
    pub fn new(packages: &ModePackages, players: u32) -> Golden {
        let names = &packages.data().resources;
        let resources = names
            .iter()
            .map(|name| ResourceId::named(names, name.as_str()).expect("a declared resource"))
            .collect();
        Golden {
            resources,
            players,
            ticks: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Records the tick `runner` just ran.
    pub fn record(&mut self, runner: &Runner) {
        self.record_hashed(runner, runner.state_hash());
    }

    /// Records the tick `runner` just ran, whose state hash `state` is, as a `HashTrail` that
    /// recorded the tick holds it: the state is hashed once a tick.
    pub fn record_hashed(&mut self, runner: &Runner, state: StateHash) {
        let behaviour = self.behaviour(runner);
        self.ticks.push(GoldenTick { state, behaviour });
    }

    /// The digest of what the tick `runner` just ran did, in no type's layout:
    /// - each unit by stable id: its position, team, owner, pools and whether it is dead;
    /// - each death of the tick, with its killer and assisters;
    /// - each failed script call, with its unit, hook and error;
    /// - each player's resources.
    fn behaviour(&mut self, runner: &Runner) -> StateHash {
        let world = runner.world();
        let bytes = &mut self.scratch;
        bytes.clear();
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            let unit = world.entity(entity);
            bytes.extend_from_slice(&id.get().to_le_bytes());
            if let Some(pos) = unit.get::<Position>() {
                let at = pos.get();
                for axis in [at.x, at.y, at.z] {
                    bytes.extend_from_slice(&axis.to_bits().to_le_bytes());
                }
            }
            bytes.push(unit.get::<Team>().map_or(u8::MAX, |team| team.get()));
            let owner = unit
                .get::<Owner>()
                .map_or(u32::MAX, |owner| owner.slot().get());
            bytes.extend_from_slice(&owner.to_le_bytes());
            if let Some(pools) = unit.get::<Pools>() {
                for pool in pools.ids() {
                    let current = pools.current(pool).expect("a pool it has");
                    bytes.push(u8::try_from(pool.index()).expect("a pool index fits u8"));
                    bytes.extend_from_slice(&current.to_bits().to_le_bytes());
                }
            }
            bytes.push(u8::from(unit.contains::<Dead>()));
        }
        let ran = Tick::new(world.resource::<SimTick>().start().get().saturating_sub(1));
        if let Some(deaths) = world.get_resource::<Deaths>()
            && deaths.tick() == ran
        {
            for death in deaths.iter() {
                bytes.extend_from_slice(&death.fallen.unit.get().to_le_bytes());
                let killer = death.killer.map_or(u64::MAX, StableId::get);
                bytes.extend_from_slice(&killer.to_le_bytes());
                for assister in death.assisters {
                    bytes.extend_from_slice(&assister.get().to_le_bytes());
                }
            }
        }
        if let Some(failures) = world.get_non_send::<ScriptFailures>() {
            for failure in failures.get() {
                let unit = failure.unit.map_or(u64::MAX, StableId::get);
                bytes.extend_from_slice(&unit.to_le_bytes());
                write!(Text(bytes), "{} {}", failure.hook.name(), failure.error)
                    .expect("text writes into bytes");
            }
        }
        if let Some(amounts) = world.get_resource::<PlayerResources>() {
            for slot in (0..self.players).map(PlayerSlot::new) {
                for &resource in &self.resources {
                    bytes.extend_from_slice(&amounts.amount(slot, resource).to_le_bytes());
                }
            }
        }
        StateRegistry::digest(bytes)
    }

    /// The most bytes a golden file may hold to be read whole: a line of about 40 bytes for each
    /// tick, so a mebibyte holds a match of 25 000 ticks, past any the tests play.
    const MAX_LEN: usize = 1 << 20;

    /// Compares the record with the golden file `name` of the runner's tests, or writes the file
    /// with `CAMPFIRE_BLESS=1`. A difference names the first tick each column differs in.
    pub fn check(&self, name: &str) {
        let path = Golden::path(name);
        let mut lines = String::new();
        for (tick, at) in self.ticks.iter().enumerate() {
            let state = &at.state.to_string()[..DIGITS];
            let behaviour = &at.behaviour.to_string()[..DIGITS];
            writeln!(lines, "{tick} {state} {behaviour}").expect("text writes into a string");
        }
        if env::var_os("CAMPFIRE_BLESS").is_some() {
            DurableFile::write(&path, lines.as_bytes())
                .unwrap_or_else(|error| panic!("{}", ErrorReport::of(&error)));
            return;
        }
        let pinned = InputFile::read_text(&path, Golden::MAX_LEN).unwrap_or_else(|error| {
            panic!(
                "{}; run with CAMPFIRE_BLESS=1 to write it",
                ErrorReport::of(&error)
            )
        });
        if pinned == lines {
            return;
        }
        let first = |column: usize| {
            pinned
                .lines()
                .zip(lines.lines())
                .position(|(was, is)| was.split(' ').nth(column) != is.split(' ').nth(column))
        };
        panic!(
            "golden {name} differs: {} ticks pinned, {} run; the state first differs at tick \
             {:?}, the behaviour at tick {:?}. A change of layout alone moves only the state, and \
             needs the snapshot's `StateRegistry::DATA_VERSION` raised; CAMPFIRE_BLESS=1 writes \
             the record again",
            pinned.lines().count(),
            self.ticks.len(),
            first(1),
            first(2),
        );
    }

    fn path(name: &str) -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"))
            .join(format!("{name}.txt"))
    }
}

/// Writes text into a byte buffer.
#[derive(Debug)]
struct Text<'a>(&'a mut Vec<u8>);

impl fmt::Write for Text<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0.extend_from_slice(text.as_bytes());
        Ok(())
    }
}
