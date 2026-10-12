use std::env;
use std::fmt::{self, Write as _};
use std::path::PathBuf;

use campfire_capabilities::PoolMeters;
use campfire_capabilities::{
    Dead, Deaths, DeclaredName, Match, Names, Owner, PlayersWorld, PoolId, ResourceId,
    ScriptFailures, Team,
};
use campfire_common::{PlayerSlot, StateHash, Tick};
use campfire_log::ErrorReport;
use campfire_package::ModePackages;
use campfire_sim::{EntityIndex, Position, SimTick, StableId, StateRegistry};
use campfire_store::InputFile;
use campfire_store::internals::PinnedFile;

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
    state_hash: StateHash,
    behaviour: StateHash,
}

/// The digits of each hash a line keeps: 64 bits tell a changed tick apart.
const DIGITS: usize = 16;

impl Golden {
    /// A record of a match of `packages` for `players` players.
    pub fn new(packages: &ModePackages, players: u32) -> Golden {
        let names = packages.data().resources.iter();
        let names: Names<ResourceId> = names.map(DeclaredName::as_str).collect();
        let resources = names.iter().map(|(resource, _)| resource).collect();
        Golden {
            resources,
            players,
            ticks: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Records the tick `game` just ran, whose state hash `state_hash` is, as the `CopyCheck` of the
    /// tick gives it: the state is hashed once a tick.
    pub fn record(&mut self, game: &Match, state_hash: StateHash) {
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
            for death in deaths.iter() {
                bytes.extend_from_slice(&death.fallen.id.get().to_le_bytes());
                let killer = death.killer.map_or(u64::MAX, StableId::get);
                bytes.extend_from_slice(&killer.to_le_bytes());
                for assister in death.assisters {
                    bytes.extend_from_slice(&assister.get().to_le_bytes());
                }
            }
        }
        if let Some(failures) = game.get_non_send::<ScriptFailures>() {
            for failure in failures.get() {
                let id = failure.id.map_or(u64::MAX, StableId::get);
                bytes.extend_from_slice(&id.to_le_bytes());
                write!(Text(bytes), "{} {}", failure.hook.name(), failure.error)
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

    /// The most bytes a golden file may hold to be read whole: a line of about 40 bytes for each
    /// tick, so a mebibyte holds a match of 25 000 ticks, past any the tests play.
    const MAX_LEN: usize = 1 << 20;

    /// Compares the record with the golden file `name` of the runner's tests, or writes the file
    /// with `CAMPFIRE_BLESS=1`. A difference names the first tick each column differs in.
    pub fn check(&self, name: &str) {
        let path = Golden::path(name);
        let mut lines = String::new();
        for (tick, at) in self.ticks.iter().enumerate() {
            let state = &at.state_hash.to_string()[..DIGITS];
            let behaviour = &at.behaviour.to_string()[..DIGITS];
            writeln!(lines, "{tick} {state} {behaviour}").expect("text writes into a string");
        }
        let blessed = PinnedFile::new(&path)
            .bless(lines.as_bytes())
            .unwrap_or_else(|error| panic!("{}", ErrorReport::of(&error)));
        if blessed {
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
