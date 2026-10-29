use std::env;
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use campfire_capabilities::Match;
use campfire_common::ErrorReport;
use campfire_common::{DeclaredName, PlayerSlot, StateHash, Tick};
use campfire_package::ModePackages;
use campfire_players::{PlayersWorld, ResourcesData};
use campfire_script::ScriptFailures;
use campfire_sim::{EntityIndex, Position, SimTick, StableId, StateRegistry};
use campfire_stats::PoolMeters;
use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;
use campfire_units::{Dead, Deaths, Owner, PoolId};
use campfire_values::{NameId, Names, ResourceId, Team};

use crate::golden::golden_units::GoldenUnits;

mod golden_units;

/// The pinned record of a whole match, one line a tick: the state hash, which changes with the
/// state's layout, and a digest of the match's behaviour, which does not. A change of layout
/// alone moves only the first column; a change of behaviour moves the second, and names itself.
/// Beside it, a second file holds the units in samples at a period, so a change of behaviour also
/// names the units it moved. `CAMPFIRE_BLESS=1` writes both again.
#[derive(Debug)]
pub(crate) struct Golden {
    resources: Vec<ResourceId>,
    players: u32,
    ticks: Vec<GoldenTick>,
    units: GoldenUnits,
    /// The bytes of one tick's behaviour, kept between ticks.
    scratch: Vec<u8>,
}

/// One tick of a `Golden`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GoldenTick {
    state_hash: StateHash,
    behaviour: StateHash,
}

/// The digits of each hash a line keeps: 64 bits tell a changed tick apart.
const DIGITS: usize = 16;

impl Golden {
    /// A record of a match of `packages` for `players` players.
    pub(crate) fn new(packages: &ModePackages, players: u32) -> Self {
        let data = packages.data().section_or_default::<ResourcesData>();
        let names: Names<ResourceId> = Names::unique(data.0.iter().map(DeclaredName::as_str));
        let resources = names.iter().map(|(resource, _)| resource).collect();
        Self {
            resources,
            players,
            ticks: Vec::new(),
            units: GoldenUnits::default(),
            scratch: Vec::new(),
        }
    }

    /// Records the tick `game` just ran, whose state hash `state_hash` is, as the `CopyCheck` of
    /// the tick gives it: the state is hashed once a tick.
    pub(crate) fn record(&mut self, game: &Match, state_hash: StateHash) {
        let tick = u64::try_from(self.ticks.len()).expect("a tick count fits u64");
        if tick % GoldenUnits::EVERY == 0 {
            self.units.record(game, tick, self.players, &self.resources);
        }
        let behaviour = self.behaviour(game);
        self.ticks.push(GoldenTick {
            state_hash,
            behaviour,
        });
    }

    /// The digest of what the tick `game` just ran did, in no type's layout:
    /// - each unit by stable id: its position, team, owner, pools by id, its life among them, and
    ///   whether it is dead; the match's and the players' entities, of named ids, by what they
    ///   hold below;
    /// - each death of the tick, with its killer and assisters;
    /// - each failed script call, with its unit, hook and error;
    /// - each player's resources.
    fn behaviour(&mut self, game: &Match) -> StateHash {
        let bytes = &mut self.scratch;
        bytes.clear();
        let units = game.resource::<EntityIndex>().iter();
        for (id, entity) in units.filter(|(id, _)| id.is_allocated()) {
            let unit = game.entity(entity);
            bytes.extend_from_slice(&id.get().to_le_bytes());
            if let Some(position) = unit.get::<Position>() {
                let at = position.get();
                for axis in [at.x, at.y, at.z] {
                    bytes.extend_from_slice(&axis.to_bits().to_le_bytes());
                }
            }
            bytes.push(unit.get::<Team>().map_or(u8::MAX, |team| team.get()));
            let owner = unit
                .get::<Owner>()
                .map_or(u32::MAX, |owner| owner.player_slot().get());
            bytes.extend_from_slice(&owner.to_le_bytes());
            let meters = PoolMeters::of(game, entity);
            for pool in (0..).map_while(PoolId::new) {
                if let Some(current) = meters.current(pool) {
                    bytes.push(u8::try_from(pool.index()).expect("a pool index fits u8"));
                    bytes.extend_from_slice(&current.to_bits().to_le_bytes());
                }
            }
            bytes.push(u8::from(unit.contains::<Dead>()));
        }
        let ran = Tick::new(game.resource::<SimTick>().start().get().saturating_sub(1));
        if let Some(deaths) = game.get_resource::<Deaths>()
            && deaths.tick() == ran
        {
            for death in deaths {
                bytes.extend_from_slice(&death.fallen.id.get().to_le_bytes());
                let killer = death.killer.map_or(u64::MAX, StableId::get);
                bytes.extend_from_slice(&killer.to_le_bytes());
                for assister in death.assisters {
                    bytes.extend_from_slice(&assister.get().to_le_bytes());
                }
            }
        }
        if let Some(failures) = game.get_resource::<ScriptFailures>() {
            for failure in failures.get() {
                let id = failure.id.map_or(u64::MAX, StableId::get);
                bytes.extend_from_slice(&id.to_le_bytes());
                let text = failures.text(failure);
                write!(Text(bytes), "{} {}", failure.hook.name, text.message())
                    .expect("text writes into bytes");
            }
        }
        for slot in (0..self.players).map(PlayerSlot::new) {
            let Some(amounts) = game.player_resources(slot) else {
                continue;
            };
            for &resource in &self.resources {
                bytes.extend_from_slice(&amounts.amount(resource).to_le_bytes());
            }
        }
        StateRegistry::digest(bytes)
    }

    /// The most bytes a golden file or its samples may hold to be read whole: a line of about 40
    /// bytes for each tick, so a mebibyte holds a match of 25 000 ticks, past any the tests play,
    /// and the samples of the longest, the 3v3 match, take a fifth of it.
    const MAX_LEN: usize = 1 << 20;

    /// Compares the record with the golden files `name` of the harness's tests, or writes them
    /// with `CAMPFIRE_BLESS=1`. A difference names the first tick each column differs in, and the
    /// units of the first sample that differs.
    pub(crate) fn check(&self, name: &str) {
        let lines = self.lines();
        let bless = |path: &Path, text: &str| {
            PinnedFile::new(path)
                .bless(text.as_bytes())
                .unwrap_or_else(|error| panic!("{}", ErrorReport::of(&error)))
        };
        let blessed = bless(&Self::path(name), &lines);
        let units = bless(&Self::units_path(name), self.units.text());
        debug_assert_eq!(blessed, units, "a run blesses both files or neither");
        if !blessed {
            self.compare_lines(name, &lines);
        }
    }

    /// Compares the record with the golden file `name`, which another test of the same match
    /// writes, and never writes it, so no two tests write one file at once: with
    /// `CAMPFIRE_BLESS=1` it compares nothing, as the file it reads is being written again.
    pub(crate) fn compare(&self, name: &str) {
        if !PinnedFile::is_blessing() {
            self.compare_lines(name, &self.lines());
        }
    }

    /// The record's lines as its golden file holds them: each tick, its state hash and its
    /// behaviour digest, each cut to its first digits.
    fn lines(&self) -> String {
        let mut lines = String::new();
        for (tick, at) in self.ticks.iter().enumerate() {
            let state = &at.state_hash.to_string()[..DIGITS];
            let behaviour = &at.behaviour.to_string()[..DIGITS];
            writeln!(lines, "{tick} {state} {behaviour}").expect("text writes into a string");
        }
        lines
    }

    /// Asserts that the golden files `name` hold `lines` and the record's units, naming the
    /// first tick each column differs in, and the units of the first sample that differs.
    fn compare_lines(&self, name: &str, lines: &str) {
        let read = |path: &Path| {
            InputFile::read_text(path, Self::MAX_LEN).unwrap_or_else(|error| {
                panic!(
                    "{}; run with CAMPFIRE_BLESS=1 to write it",
                    ErrorReport::of(&error)
                )
            })
        };
        let pinned = read(&Self::path(name));
        let pinned_units = read(&Self::units_path(name));
        let moved = GoldenUnits::moved(&pinned_units, self.units.text());
        if pinned == lines && moved.is_none() {
            return;
        }
        let first = |column: usize| {
            pinned
                .lines()
                .zip(lines.lines())
                .position(|(was, is)| was.split(' ').nth(column) != is.split(' ').nth(column))
                .map_or_else(|| "no tick".to_owned(), |tick| format!("tick {tick}"))
        };
        panic!(
            "golden {name} differs: {} ticks pinned, {} run; the state first differs at {}, the \
             behaviour at {}. A change of layout alone moves only the state, \
             and needs the snapshot's `StateRegistry::DATA_VERSION` raised; CAMPFIRE_BLESS=1 \
             writes the record again.\n{}",
            pinned.lines().count(),
            self.ticks.len(),
            first(1),
            first(2),
            moved.map_or_else(String::new, |moved| moved.to_string()),
        );
    }

    fn path(name: &str) -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"))
            .join(format!("{name}.txt"))
    }

    fn units_path(name: &str) -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"))
            .join(format!("{name}.units.txt"))
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
