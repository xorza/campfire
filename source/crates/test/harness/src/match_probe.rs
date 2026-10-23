use std::fmt::Write as _;
use std::num::{NonZeroU32, NonZeroU64, ParseIntError};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::{env, thread};

use bevy_ecs::world::{EntityRef, World};
use campfire_actions::{ActionProgress, ActionTarget, InProgress};
use campfire_common::{DeclaredName, ErrorReport, PlayerSlot, StableNumber};
use campfire_math::Vec3;
use campfire_navigation::{Progress, Route};
use campfire_protocol::SessionId;
use campfire_sim::{MatchRate, Position, SimTick, StableId};
use campfire_store::{DurableDir, DurableFile};
use campfire_units::{AtWork, Destination, Life, Owner, UnitTypeBook};
use campfire_values::{Names, Team, TypeScope, UnitType};

use crate::error::{ProbeRefusal, ProbeSpecError, ProbeWriteError};
use crate::match_units::MatchUnits;

/// A table of a match's units at a period, as text, for a person to read: for each player slot
/// the counts of the units it selects that the slot owns, by what they do, then a line for each
/// unit it selects. A test
/// asserts on its lines, and a check writes them to a file, in place of a dump written into a
/// play and removed after (02-engine-core.md, One probe of a match).
#[derive(Debug)]
pub struct MatchProbe {
    every: NonZeroU64,
    units: ProbeUnits,
    text: String,
}

/// The units a probe prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeUnits {
    All,
    Owner(PlayerSlot),
    Team(Team),
    Type(UnitType),
    /// In any order.
    Ids(Vec<StableId>),
}

/// A probe as text names it, `<period_ms>:<units>`, as `CAMPFIRE_PROBE` and the Zero Hour
/// check's `--probe` take it: how often it writes, and the units it prints, `all`,
/// `player=<n>` counted from 1, as a check names its players, `team=<n>`, `type=<name>` or
/// `ids=<id>,<id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSpec {
    pub every_ms: NonZeroU64,
    pub units: ProbeUnitsSpec,
}

/// The units a probe's text names, a type by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeUnitsSpec {
    All,
    Owner(PlayerSlot),
    Team(Team),
    Type(DeclaredName),
    Ids(Vec<StableId>),
}

/// A probe of a match, and the file it writes.
#[derive(Debug)]
pub struct ProbeFile {
    pub probe: MatchProbe,
    pub file: PathBuf,
}

/// What a unit does, as a probe reads it: it fights while it holds an attack target or attacks,
/// acts while another action is under way, works while its building goes on, walks while it has
/// a destination, and else stands idle: a walker idle here is one steering takes as an idle
/// friend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Doing {
    Idle,
    Walking,
    Working,
    Fighting,
    Acting,
}

/// The counts of one player slot's units by what they do, and of its walkers that are stuck.
#[derive(Debug, Default)]
struct Counts {
    idle: usize,
    walking: usize,
    stuck: usize,
    working: usize,
    fighting: usize,
    acting: usize,
}

/// The table's columns, and whether each aligns right, as a number does.
const COLUMNS: [(&str, bool); 12] = [
    ("id", true),
    ("type", false),
    ("slot", true),
    ("team", true),
    ("x", true),
    ("z", true),
    ("doing", false),
    ("target", false),
    ("destination", false),
    ("goal", false),
    ("stuck", true),
    ("life", true),
];

impl MatchProbe {
    /// A probe of `units` every `every` ticks, from tick 0.
    pub fn new(every: NonZeroU64, units: ProbeUnits) -> Self {
        let units = match units {
            ProbeUnits::Ids(mut ids) => {
                ids.sort_unstable();
                ids.dedup();
                ProbeUnits::Ids(ids)
            }
            units => units,
        };
        Self {
            every,
            units,
            text: String::new(),
        }
    }

    /// What it wrote so far.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Writes the table of `world` when the tick it just ran falls on the period; nothing before
    /// its first tick.
    pub fn record(&mut self, world: &World) {
        let Some(ran) = world.resource::<SimTick>().start().get().checked_sub(1) else {
            return;
        };
        if ran % self.every != 0 {
            return;
        }
        let rate = *world.resource::<MatchRate>();
        let stuck_after = Progress::stuck_after(rate);
        let names = world
            .get_resource::<UnitTypeBook>()
            .map(UnitTypeBook::types);
        let mut counts: Vec<(PlayerSlot, Counts)> = Vec::new();
        let mut rows: Vec<[String; COLUMNS.len()]> = Vec::new();
        for (id, unit) in MatchUnits::of(world).all() {
            if !id.is_allocated() || !self.selects(id, &unit) {
                continue;
            }
            let doing = Self::doing(&unit);
            let stuck = unit.get::<Progress>().map(Progress::stuck_ticks);
            if let Some(owner) = unit.get::<Owner>() {
                let slot = owner.player_slot();
                let at = counts.iter().position(|&(of, _)| of == slot);
                let at = at.unwrap_or_else(|| {
                    counts.push((slot, Counts::default()));
                    counts.len() - 1
                });
                let stuck = doing == Doing::Walking && stuck.is_some_and(|s| s >= stuck_after);
                counts[at].1.count(doing, stuck);
            }
            rows.push(Self::row(id, &unit, doing, stuck, names));
        }
        counts.sort_unstable_by_key(|&(slot, _)| slot);
        let hz = NonZeroU64::from(rate.hz());
        let (seconds, rest) = (ran / hz, ran % hz);
        match rest {
            0 => writeln!(self.text, "tick {ran}, {seconds} s"),
            _ => writeln!(self.text, "tick {ran}, {seconds} s and {rest} ticks"),
        }
        .expect("text writes");
        for (slot, counts) in &counts {
            writeln!(self.text, "player slot {}: {counts}", slot.get()).expect("text writes");
        }
        self.write_table(&rows);
        self.text.push('\n');
    }

    /// Whether it prints the unit `id`.
    fn selects(&self, id: StableId, unit: &EntityRef<'_>) -> bool {
        match &self.units {
            ProbeUnits::All => true,
            &ProbeUnits::Owner(slot) => unit
                .get::<Owner>()
                .is_some_and(|owner| owner.player_slot() == slot),
            ProbeUnits::Team(team) => unit.get::<Team>() == Some(team),
            ProbeUnits::Type(unit_type) => unit.get::<UnitType>() == Some(unit_type),
            ProbeUnits::Ids(ids) => ids.binary_search(&id).is_ok(),
        }
    }

    fn doing(unit: &EntityRef<'_>) -> Doing {
        let progress = unit.get::<ActionProgress>();
        let underway = progress.and_then(ActionProgress::in_progress);
        let attacks = progress.and_then(ActionProgress::attack_target).is_some()
            || matches!(underway, Some(InProgress::Attack { .. }));
        let walks = unit
            .get::<Destination>()
            .is_some_and(|destination| destination.get().is_some());
        if attacks {
            Doing::Fighting
        } else if underway.is_some() {
            Doing::Acting
        } else if unit.get::<AtWork>().is_some_and(|at_work| at_work.get()) {
            Doing::Working
        } else if walks {
            Doing::Walking
        } else {
            Doing::Idle
        }
    }

    /// The cells of the unit `id`'s line.
    fn row(
        id: StableId,
        unit: &EntityRef<'_>,
        doing: Doing,
        stuck: Option<u32>,
        names: Option<&Arc<Names<UnitType>>>,
    ) -> [String; COLUMNS.len()] {
        let none = || "-".to_owned();
        let at = unit.get::<Position>().map(|position| position.get());
        let unit_type = unit.get::<UnitType>().copied();
        let named = unit_type.and_then(|unit_type| names?.name(unit_type));
        let progress = unit.get::<ActionProgress>();
        let underway = progress.and_then(ActionProgress::in_progress);
        let target = progress
            .and_then(ActionProgress::attack_target)
            .map_or_else(
                || underway.and_then(Self::aimed).unwrap_or_else(none),
                |target| format!("#{}", target.get()),
            );
        [
            id.get().to_string(),
            match (named, unit_type) {
                (Some(name), _) => name.to_owned(),
                (None, Some(unit_type)) => format!("{unit_type:?}"),
                (None, None) => none(),
            },
            unit.get::<Owner>()
                .map_or_else(none, |owner| owner.player_slot().get().to_string()),
            unit.get::<Team>()
                .map_or_else(none, |team| team.get().to_string()),
            at.map_or_else(none, |at| format!("{:?}", at.x)),
            at.map_or_else(none, |at| format!("{:?}", at.z)),
            Self::doing_text(doing, underway),
            target,
            unit.get::<Destination>()
                .and_then(|destination| destination.get())
                .map_or_else(none, Self::ground),
            unit.get::<Route>()
                .and_then(Route::goal)
                .map_or_else(none, Self::ground),
            stuck.map_or_else(none, |stuck| stuck.to_string()),
            unit.get::<Life>().map_or_else(none, |life| {
                format!("{:?}/{:?}", life.current(), life.max())
            }),
        ]
    }

    /// What a unit does, with the action under way and its slot.
    fn doing_text(doing: Doing, underway: Option<InProgress>) -> String {
        let action = match underway {
            Some(InProgress::Attack { slot, .. }) => format!("attack {slot}"),
            Some(InProgress::Order { aim, .. }) => format!("order {}", aim.slot),
            Some(InProgress::Charge { aim, .. }) => format!("charge {}", aim.slot),
            Some(InProgress::Channel { aim, .. }) => format!("channel {}", aim.slot),
            Some(InProgress::WindDown { slot, .. }) => format!("wind-down {slot}"),
            None => String::new(),
        };
        let doing = match doing {
            Doing::Idle => "idle",
            Doing::Walking => "walk",
            Doing::Working => "work",
            Doing::Fighting => "fight",
            Doing::Acting => "act",
        };
        if action.is_empty() {
            doing.to_owned()
        } else {
            format!("{doing} {action}")
        }
    }

    /// The target of an action under way: a unit by its stable id, or a point.
    fn aimed(underway: InProgress) -> Option<String> {
        let aim = match underway {
            InProgress::Order { aim, .. }
            | InProgress::Charge { aim, .. }
            | InProgress::Channel { aim, .. } => aim,
            InProgress::Attack { target, .. } => return Some(format!("#{}", target.get())),
            InProgress::WindDown { .. } => return None,
        };
        match aim.target {
            ActionTarget::None => None,
            ActionTarget::Unit(unit) => Some(format!("#{}", unit.get())),
            ActionTarget::Point(point) => Some(Self::ground(point)),
        }
    }

    /// A point on the ground plane, `x, z`.
    fn ground(at: Vec3) -> String {
        format!("{:?}, {:?}", at.x, at.z)
    }

    /// Writes `rows` under the columns' names, each column as wide as its widest cell.
    fn write_table(&mut self, rows: &[[String; COLUMNS.len()]]) {
        let mut widths = COLUMNS.map(|(name, _)| name.chars().count());
        for row in rows {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.chars().count());
            }
        }
        let names = COLUMNS.map(|(name, _)| name.to_owned());
        for row in std::iter::once(&names).chain(rows) {
            let mut line = String::new();
            for ((cell, width), (_, right)) in row.iter().zip(widths).zip(COLUMNS) {
                if !line.is_empty() {
                    line.push_str("  ");
                }
                if right {
                    write!(line, "{cell:>width$}").expect("text writes");
                } else {
                    write!(line, "{cell:<width$}").expect("text writes");
                }
            }
            self.text.push_str(line.trim_end());
            self.text.push('\n');
        }
    }
}

impl ProbeSpec {
    /// The variable that asks every fixed match for a probe.
    pub(crate) const VAR: &'static str = "CAMPFIRE_PROBE";

    /// Its probe of a match of `types` at `hz` ticks a second; a refusal for a period of no
    /// whole count of ticks, which a probe would round, or a type the match has not.
    pub fn probe(
        &self,
        types: Option<&UnitTypeBook>,
        hz: NonZeroU32,
    ) -> Result<MatchProbe, ProbeRefusal> {
        let scaled = self.every_ms.checked_mul(NonZeroU64::from(hz));
        let every = scaled
            .filter(|scaled| scaled.get() % 1000 == 0)
            .and_then(|scaled| NonZeroU64::new(scaled.get() / 1000))
            .ok_or(ProbeRefusal::Period(self.every_ms))?;
        let units = match &self.units {
            ProbeUnitsSpec::All => ProbeUnits::All,
            &ProbeUnitsSpec::Owner(slot) => ProbeUnits::Owner(slot),
            &ProbeUnitsSpec::Team(team) => ProbeUnits::Team(team),
            ProbeUnitsSpec::Type(name) => {
                let found =
                    types.and_then(|types| types.types().named((TypeScope::Mode, name.as_str())));
                ProbeUnits::Type(found.ok_or_else(|| ProbeRefusal::NoType(name.clone()))?)
            }
            ProbeUnitsSpec::Ids(ids) => ProbeUnits::Ids(ids.clone()),
        };
        Ok(MatchProbe::new(every, units))
    }
}

impl FromStr for ProbeSpec {
    type Err = ProbeSpecError;

    fn from_str(text: &str) -> Result<Self, ProbeSpecError> {
        let (every_ms, units) = text.split_once(':').ok_or(ProbeSpecError::Shape)?;
        Ok(Self {
            every_ms: every_ms.parse().map_err(ProbeSpecError::Period)?,
            units: units.parse()?,
        })
    }
}

impl FromStr for ProbeUnitsSpec {
    type Err = ProbeSpecError;

    fn from_str(text: &str) -> Result<Self, ProbeSpecError> {
        if text == "all" {
            return Ok(Self::All);
        }
        let (kind, value) = text.split_once('=').ok_or(ProbeSpecError::Units)?;
        Ok(match kind {
            "player" => {
                let player: u32 = number(value)?;
                let index = player.checked_sub(1).ok_or(ProbeSpecError::Player)?;
                Self::Owner(PlayerSlot::new(index))
            }
            "team" => Self::Team(Team::new(number(value)?)),
            "type" => Self::Type(DeclaredName::new(value).ok_or(ProbeSpecError::Name)?),
            "ids" => {
                let ids: Result<Vec<u64>, _> = value.split(',').map(number).collect();
                let ids = ids?.into_iter();
                Self::Ids(
                    ids.map(|id| StableId::from(StableNumber::new(id)))
                        .collect(),
                )
            }
            _ => return Err(ProbeSpecError::Units),
        })
    }
}

/// The number `text` names.
fn number<T: FromStr<Err = ParseIntError>>(text: &str) -> Result<T, ProbeSpecError> {
    text.parse().map_err(ProbeSpecError::Number)
}

impl ProbeFile {
    /// The probe `CAMPFIRE_PROBE` asks of `game`, a match of the session `session`, which writes
    /// `target/probe/<test>-<session>.probe`, its test the thread it runs on, as libtest names
    /// it, its session the first digits of its id; none with no variable. A variable that does
    /// not read, or a probe the match refuses, is a test's misuse, and fails it.
    pub(crate) fn of_env(game: &World, session: SessionId) -> Option<Self> {
        let text = env::var(ProbeSpec::VAR).ok()?;
        let spec: ProbeSpec = text.parse().unwrap_or_else(|error| {
            panic!("{}={text}: {}", ProbeSpec::VAR, ErrorReport::of(&error))
        });
        let types = game.get_resource::<UnitTypeBook>();
        let hz = game.resource::<MatchRate>().hz();
        let probe = spec.probe(types, hz).unwrap_or_else(|error| {
            panic!("{}={text}: {}", ProbeSpec::VAR, ErrorReport::of(&error))
        });
        let thread = thread::current();
        let test = thread.name().unwrap_or("main").replace("::", "-");
        let session = &session.to_string()[..8];
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../target/probe"
        ));
        Some(Self {
            probe,
            file: dir.join(format!("{test}-{session}.probe")),
        })
    }

    /// Writes what the probe wrote to its file, its directory made if it is missing.
    pub fn write(&self) -> Result<(), ProbeWriteError> {
        if let Some(dir) = self.file.parent() {
            DurableDir::create_all(dir)?;
        }
        DurableFile::write(&self.file, self.probe.text().as_bytes())?;
        Ok(())
    }
}

impl Counts {
    const fn count(&mut self, doing: Doing, stuck: bool) {
        let counted = match doing {
            Doing::Idle => &mut self.idle,
            Doing::Walking => &mut self.walking,
            Doing::Working => &mut self.working,
            Doing::Fighting => &mut self.fighting,
            Doing::Acting => &mut self.acting,
        };
        *counted += 1;
        if stuck {
            self.stuck += 1;
        }
    }
}

impl std::fmt::Display for Counts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            idle,
            walking,
            stuck,
            working,
            fighting,
            acting,
        } = self;
        write!(
            f,
            "{idle} idle, {walking} walking ({stuck} stuck), {working} working, \
             {fighting} fighting, {acting} acting"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_reads_its_period_and_its_units() {
        let every = |ms| NonZeroU64::new(ms).unwrap();
        let id = |id| StableId::from(StableNumber::new(id));
        let name = |text| DeclaredName::new(text).unwrap();
        for (text, units) in [
            ("1000:all", ProbeUnitsSpec::All),
            // Players count from 1, as the check names them: player 2 is slot 1.
            ("1000:player=2", ProbeUnitsSpec::Owner(PlayerSlot::new(1))),
            ("1000:team=3", ProbeUnitsSpec::Team(Team::new(3))),
            ("1000:type=dozer", ProbeUnitsSpec::Type(name("dozer"))),
            ("1000:ids=9,2", ProbeUnitsSpec::Ids(vec![id(9), id(2)])),
        ] {
            let expected = ProbeSpec {
                every_ms: every(1000),
                units,
            };
            assert_eq!(text.parse(), Ok(expected), "{text}");
        }
        for (text, error) in [
            ("1000", ProbeSpecError::Shape),
            ("1000:some", ProbeSpecError::Units),
            ("1000:player=0", ProbeSpecError::Player),
            ("1000:type=", ProbeSpecError::Name),
        ] {
            assert_eq!(text.parse::<ProbeSpec>(), Err(error), "{text}");
        }
        for text in [
            "0:all",
            "soon:all",
            "1000:team=x",
            "1000:ids=1,,2",
            "1000:player=-1",
        ] {
            let refused = text.parse::<ProbeSpec>();
            let number = matches!(
                refused,
                Err(ProbeSpecError::Period(_) | ProbeSpecError::Number(_))
            );
            assert!(number, "{text}: {refused:?}");
        }
    }
}
