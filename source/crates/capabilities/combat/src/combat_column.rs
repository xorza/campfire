use std::ops::Range;

use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_actions::Targets;
use campfire_declarations::ById;
use campfire_script::{
    AnyKeptRows, Checked, ColumnRows, KeptRows, RowFill, RunMove, ScriptList, ScriptValue,
    ScriptWindow, Unit, ViewColumn,
};
use campfire_units::{Dead, Life};
use campfire_values::{Team, UnitTags, UnitType};
use campfire_visibility::DataKind;

use crate::on_death::OnDeath;
use crate::recent_attack::RecentAttack;
use crate::recent_attackers::RecentAttackers;

/// The parts of a unit combat reads into its row: whether it is dead, its life and tags, and who
/// struck it recently.
pub(crate) type RowReads = (
    Has<Dead>,
    Option<&'static Life>,
    Option<&'static UnitTags>,
    Option<&'static RecentAttackers>,
);

/// What combat adds to the script view: the unit types whose units stay when they die, as each
/// read takes them of the types' book, and the recent attacks on each unit, a run for each row,
/// in one buffer.
#[derive(Debug, Default)]
pub struct CombatColumn {
    rows: KeptRows<CombatRows>,
    /// In the order of their ids.
    staying: Vec<UnitType>,
}

/// The rows of one read of the combat column.
#[derive(Debug, Default, PartialEq, Eq)]
struct CombatRows {
    attacks: Vec<RecentAttack>,
    /// Where each row's run starts.
    starts: Vec<u32>,
}

impl ViewColumn for CombatColumn {
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, world: &World) -> bool {
        self.staying.clear();
        let deaths = world.get_resource::<ById<UnitType, OnDeath>>();
        let types = deaths.into_iter().flat_map(ById::iter);
        let staying = types.filter(|&(_, &on_death)| on_death == OnDeath::Stay);
        self.staying.extend(staying.map(|(unit_type, _)| unit_type));
        false
    }

    /// None: it reads no team's sight.
    fn hides(&self, _: usize, _: Team) -> bool {
        false
    }
}

impl ColumnRows for CombatRows {
    fn clear(&mut self) {
        self.attacks.clear();
        self.starts.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        let first = from.starts[rows.start];
        let end = from.end(rows.end - 1);
        let moved = RunMove::new(first, self.attacks.len());
        let starts = from.starts[rows].iter();
        self.starts.extend(starts.map(|&start| moved.at(start)));
        self.attacks
            .extend_from_slice(&from.attacks[first as usize..end as usize]);
    }

    fn len(&self) -> usize {
        self.starts.len()
    }
}

impl CombatRows {
    fn push(&mut self, attacks: impl IntoIterator<Item = RecentAttack>) {
        self.starts
            .push(u32::try_from(self.attacks.len()).expect("attacks fit u32"));
        self.attacks.extend(attacks);
    }

    /// The recent attacks on the unit in row `row`.
    fn run(&self, row: usize) -> &[RecentAttack] {
        &self.attacks[self.starts[row] as usize..self.end(row) as usize]
    }

    /// Where the run of row `row` ends.
    fn end(&self, row: usize) -> u32 {
        let end = self.starts.get(row + 1).copied();
        end.unwrap_or_else(|| u32::try_from(self.attacks.len()).expect("attacks fit u32"))
    }
}

impl CombatColumn {
    /// Adds the row of a unit that `attacks` struck recently.
    pub fn push(&mut self, attacks: impl IntoIterator<Item = RecentAttack>) {
        self.rows.now_mut().push(attacks);
    }

    /// Whether `unit` stays when it dies, for the mode to bring back, as its type says; none does
    /// in a view with no combat.
    pub(crate) fn stays(unit: &Unit) -> Checked<bool> {
        let unit_type = unit.read(|row| row.unit_type)?;
        let view = unit.view();
        let stays = view.column(|column: &Self| column.staying.binary_search(&unit_type).is_ok());
        Ok(stays.unwrap_or(false))
    }

    /// `unit.recent_attackers(ms)`: the living units that struck `unit` within the last `ms`
    /// milliseconds, rounded up to whole ticks, by stable id; none in a view with no combat.
    pub(crate) fn recent_attackers(unit: &Unit, ms: ScriptWindow) -> Checked<ScriptList> {
        let view = unit.view();
        let (window, now) = (ms.get(), view.now());
        let row = unit.admit(DataKind::Server)?;
        let attackers = view.column(|column: &Self| {
            column
                .rows
                .now()
                .run(row)
                .iter()
                // A strike later than the view's tick, as a rollback can leave, is not recent.
                .filter(|attack| now.since(attack.tick).is_some_and(|age| age <= window))
                .filter_map(|attack| view.unit(attack.source))
                .filter(|attacker| {
                    let alive = attacker.read(|row| row.alive);
                    alive.expect("a unit of the view has its row")
                })
                .map(ScriptValue::of)
                .collect()
        });
        Ok(attackers.unwrap_or_default())
    }

    /// Fills a row of the script view with what combat holds: whether the unit lives and whether it
    /// may be a target, in the core's row; who struck it recently, in combat's column.
    pub(super) fn fill_row(parts: ROQueryItem<'_, '_, RowReads>, fill: &mut RowFill<'_, Self>) {
        let (dead, life, tags, recent) = parts;
        fill.row.alive = campfire_units::lives(dead, life);
        fill.row.targetable = Targets::targetable(dead, life, tags);
        fill.column
            .push(recent.into_iter().flat_map(RecentAttackers::iter));
    }
}
