use std::fmt::{self, Write as _};
use std::num::NonZeroU64;
use std::sync::Arc;

use bevy_ecs::world::{EntityRef, World};
use campfire_actions::{ActionProgress, ActionTarget, InProgress};
use campfire_common::PlayerSlot;
use campfire_math::Vec3;
use campfire_navigation::{Progress, Route};
use campfire_production::{Activity, ActivityParts};
use campfire_sim::{EntityIndex, MatchRate, Position, SimTick, StableId};
use campfire_units::{Destination, Life, Owner, UnitTypeBook};
use campfire_values::{Names, Team, UnitType};

pub(crate) mod error;
pub(crate) mod probe_spec;

/// A table of a match's units at a period, as text, for a person to read: for each player slot
/// the counts of the units it selects that the slot owns, by what they do, then a line for each
/// unit it selects. A test asserts on its lines, a check writes them to a file, and the verifier
/// tables a report's tick, in place of a dump written into a play and removed after
/// (02-engine-core.md, One probe of a match).
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
    State(ProbeState),
}

/// What a unit does, as a probe reads it, which a probe selects by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeState {
    Idle,
    /// Walks, stuck or not.
    Walking,
    /// Walks, and its walk made no progress for longer than steering waits.
    Stuck,
    Working,
    Fighting,
    Acting,
}

/// The columns of a probe's table: each, or a unit's id, type, player slot, place, what it does
/// and its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeColumns {
    Concise,
    Detailed,
}

/// The part of a table a probe writes: its columns, and its rows from the row `first`, counted
/// from 0, at most `limit` of them, every row after `first` when none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbePage {
    pub columns: ProbeColumns,
    pub first: usize,
    pub limit: Option<usize>,
}

/// The rows of a table: of the units its probe selects, and of those it wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeRows {
    pub selected: usize,
    pub shown: usize,
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

/// Whether a concise table holds each column: the id, the type, the player slot, the place, what
/// the unit does and its life.
const CONCISE: [bool; COLUMNS.len()] = [
    true, true, true, false, true, true, true, false, false, false, false, true,
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
        if ran % self.every == 0 {
            self.write(world, Some(ran), ProbePage::WHOLE);
        }
    }

    /// Writes the table of `world` as it stands, whatever the period, as a report's replay
    /// tables the tick its client drew.
    pub fn table(&mut self, world: &World) {
        self.page(world, ProbePage::WHOLE);
    }

    /// Writes the part `page` of the table of `world` as it stands, as a console asks for it.
    pub fn page(&mut self, world: &World, page: ProbePage) -> ProbeRows {
        let ran = world.resource::<SimTick>().start().get().checked_sub(1);
        self.write(world, ran, page)
    }

    /// Writes the part `page` of the table of `world` after the tick `ran`, or before its first.
    fn write(&mut self, world: &World, ran: Option<u64>, page: ProbePage) -> ProbeRows {
        let rate = *world.resource::<MatchRate>();
        let stuck_after = Progress::stuck_after(rate);
        let names = world
            .get_resource::<UnitTypeBook>()
            .map(UnitTypeBook::types);
        let mut counts: Vec<(PlayerSlot, Counts)> = Vec::new();
        let mut rows: Vec<[String; COLUMNS.len()]> = Vec::new();
        let mut selected = 0;
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            if !id.is_allocated() {
                continue;
            }
            let unit = world.entity(entity);
            let doing = Self::doing(&unit);
            let stuck = unit.get::<Progress>().map(Progress::stuck_ticks);
            let walks_stuck = Self::walks_stuck(doing, stuck, stuck_after);
            if !self.selects(id, &unit, doing, walks_stuck) {
                continue;
            }
            if let Some(owner) = unit.get::<Owner>() {
                let slot = owner.player_slot();
                let at = counts.iter().position(|&(of, _)| of == slot);
                let at = at.unwrap_or_else(|| {
                    counts.push((slot, Counts::default()));
                    counts.len() - 1
                });
                counts[at].1.count(doing, walks_stuck);
            }
            if page.shows(selected) {
                rows.push(Self::row(id, &unit, doing, stuck, names));
            }
            selected += 1;
        }
        counts.sort_unstable_by_key(|&(slot, _)| slot);
        let hz = NonZeroU64::from(rate.hz());
        match ran.map(|ran| (ran, ran / hz, ran % hz)) {
            None => writeln!(self.text, "before tick 0"),
            Some((ran, seconds, 0)) => writeln!(self.text, "tick {ran}, {seconds} s"),
            Some((ran, seconds, rest)) => {
                writeln!(self.text, "tick {ran}, {seconds} s and {rest} ticks")
            }
        }
        .expect("text writes");
        for (slot, counts) in &counts {
            writeln!(self.text, "player slot {}: {counts}", slot.get()).expect("text writes");
        }
        self.write_table(&rows, page.columns);
        self.text.push('\n');
        ProbeRows {
            selected,
            shown: rows.len(),
        }
    }

    /// Whether it prints the unit `id`, which does `doing`, a walker stuck when `walks_stuck`.
    fn selects(
        &self,
        id: StableId,
        unit: &EntityRef<'_>,
        doing: Activity,
        walks_stuck: bool,
    ) -> bool {
        match &self.units {
            ProbeUnits::All => true,
            &ProbeUnits::Owner(slot) => unit
                .get::<Owner>()
                .is_some_and(|owner| owner.player_slot() == slot),
            ProbeUnits::Team(team) => unit.get::<Team>() == Some(team),
            ProbeUnits::Type(unit_type) => unit.get::<UnitType>() == Some(unit_type),
            ProbeUnits::Ids(ids) => ids.binary_search(&id).is_ok(),
            &ProbeUnits::State(state) => state.is(doing, walks_stuck),
        }
    }

    /// Whether a unit that does `doing`, stuck for `stuck` ticks, walks stuck: past
    /// `stuck_after`.
    fn walks_stuck(doing: Activity, stuck: Option<u32>, stuck_after: u32) -> bool {
        doing == Activity::Walking && stuck.is_some_and(|stuck| stuck >= stuck_after)
    }

    fn doing(unit: &EntityRef<'_>) -> Activity {
        ActivityParts::of(unit).activity()
    }

    /// The cells of the unit `id`'s line.
    fn row(
        id: StableId,
        unit: &EntityRef<'_>,
        doing: Activity,
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
    fn doing_text(doing: Activity, underway: Option<InProgress>) -> String {
        let action = match underway {
            Some(InProgress::Attack { slot, .. }) => format!("attack {slot}"),
            Some(InProgress::Order { aim, .. }) => format!("order {}", aim.slot),
            Some(InProgress::Charge { aim, .. }) => format!("charge {}", aim.slot),
            Some(InProgress::Channel { aim, .. }) => format!("channel {}", aim.slot),
            Some(InProgress::WindDown { slot, .. }) => format!("wind-down {slot}"),
            None => String::new(),
        };
        let doing = match doing {
            Activity::Idle => "idle",
            Activity::Walking => "walk",
            Activity::Working => "work",
            Activity::Fighting => "fight",
            Activity::Acting => "act",
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

    /// Writes the cells of `columns` of `rows` under the columns' names, each column as wide as
    /// its widest cell.
    fn write_table(&mut self, rows: &[[String; COLUMNS.len()]], columns: ProbeColumns) {
        let mut widths = COLUMNS.map(|(name, _)| name.chars().count());
        for row in rows {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.chars().count());
            }
        }
        let names = COLUMNS.map(|(name, _)| name.to_owned());
        let shown = match columns {
            ProbeColumns::Concise => CONCISE,
            ProbeColumns::Detailed => [true; COLUMNS.len()],
        };
        for row in std::iter::once(&names).chain(rows) {
            let mut line = String::new();
            let cells = row.iter().zip(widths).zip(COLUMNS).zip(shown);
            for (((cell, width), (_, right)), _) in cells.filter(|&(_, shown)| shown) {
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

impl ProbeState {
    /// Whether the unit `id` of `world` is in it, as a probe reads it; false for a unit the
    /// match lacks.
    pub fn holds(self, world: &World, id: StableId) -> bool {
        let Some(entity) = world.resource::<EntityIndex>().get(id) else {
            return false;
        };
        let unit = world.entity(entity);
        let doing = MatchProbe::doing(&unit);
        let stuck = unit.get::<Progress>().map(Progress::stuck_ticks);
        let stuck_after = Progress::stuck_after(*world.resource::<MatchRate>());
        self.is(doing, MatchProbe::walks_stuck(doing, stuck, stuck_after))
    }

    /// Whether a unit that does `doing`, a walker stuck when `walks_stuck`, is in it.
    fn is(self, doing: Activity, walks_stuck: bool) -> bool {
        match self {
            Self::Idle => doing == Activity::Idle,
            Self::Walking => doing == Activity::Walking,
            Self::Stuck => walks_stuck,
            Self::Working => doing == Activity::Working,
            Self::Fighting => doing == Activity::Fighting,
            Self::Acting => doing == Activity::Acting,
        }
    }
}

impl ProbePage {
    /// Every column and every row.
    pub const WHOLE: Self = Self {
        columns: ProbeColumns::Detailed,
        first: 0,
        limit: None,
    };

    /// Whether it shows the row `row` of a table.
    fn shows(self, row: usize) -> bool {
        row >= self.first && self.limit.is_none_or(|limit| row - self.first < limit)
    }
}

impl Counts {
    const fn count(&mut self, doing: Activity, stuck: bool) {
        let counted = match doing {
            Activity::Idle => &mut self.idle,
            Activity::Walking => &mut self.walking,
            Activity::Working => &mut self.working,
            Activity::Fighting => &mut self.fighting,
            Activity::Acting => &mut self.acting,
        };
        *counted += 1;
        if stuck {
            self.stuck += 1;
        }
    }
}

impl fmt::Display for Counts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
