use std::ops::Range;

use bevy_ecs::query::{Has, ROQueryItem};
use bevy_ecs::world::World;
use campfire_script::rhai::{Array, Dynamic, INT};

use crate::actions::targets::Targets;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attack::RecentAttack;
use crate::combat::recent_attackers::RecentAttackers;
use crate::scripts::error::{ApiError, Checked};
use crate::stats::life_pool::LifePool;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::units::dead::Dead;
use crate::units::kept_rows::{ColumnRows, KeptRows, RunMove};
use crate::units::row_fill::RowFill;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;
use crate::units::view_column::ViewColumn;

/// The parts of a unit combat reads into its row: whether it is dead, its pools and tags, what
/// it does when it dies, and who struck it recently.
pub(super) type RowParts = (
    Has<Dead>,
    Option<&'static Pools>,
    Option<&'static UnitTags>,
    Option<&'static OnDeath>,
    Option<&'static RecentAttackers>,
);

/// What combat adds to the script view: whether each unit stays when it dies, and the recent
/// attacks on it, a run for each row, in one buffer; and the mode's life pool, as the read
/// found it, which whether a unit may be a target derives from.
#[derive(Debug, Default)]
pub(crate) struct CombatColumn {
    rows: KeptRows<CombatRows>,
    life: Option<PoolId>,
}

/// The rows of one read of the combat column.
#[derive(Debug, Default, PartialEq, Eq)]
struct CombatRows {
    stays: Vec<bool>,
    attacks: Vec<RecentAttack>,
    /// Where each row's run starts.
    starts: Vec<u32>,
}

impl ViewColumn for CombatColumn {
    fn begin(&mut self, world: &World) -> bool {
        self.rows.begin();
        let life = world.get_resource::<LifePool>().map(|life| life.0);
        let changed = life != self.life;
        self.life = life;
        changed
    }

    fn keep(&mut self, rows: Range<usize>) {
        self.rows.keep(rows);
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }

    fn same_as_kept(&self) -> bool {
        self.rows.same_as_kept()
    }
}

impl ColumnRows for CombatRows {
    fn clear(&mut self) {
        self.stays.clear();
        self.attacks.clear();
        self.starts.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        let first = from.starts[rows.start];
        let end = from.end(rows.end - 1);
        let moved = RunMove::new(first, self.attacks.len());
        self.stays.extend_from_slice(&from.stays[rows.clone()]);
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
    fn push(&mut self, stays: bool, attacks: impl IntoIterator<Item = RecentAttack>) {
        self.stays.push(stays);
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
    /// Adds the row of a unit that stays when it dies when `stays`, and that `attacks` struck
    /// recently.
    pub(crate) fn push(&mut self, stays: bool, attacks: impl IntoIterator<Item = RecentAttack>) {
        self.rows.now_mut().push(stays, attacks);
    }

    /// The mode's life pool, as the read found it.
    pub(crate) const fn life(&self) -> Option<PoolId> {
        self.life
    }

    /// Whether `unit` stays when it dies, for the mode to bring back; none does in a view with no
    /// combat.
    pub(crate) fn stays(unit: &Unit) -> bool {
        let view = unit.view();
        let stays = view.column(|column: &CombatColumn| column.rows.now().stays[unit.row_index()]);
        stays.unwrap_or(false)
    }

    /// `unit.recent_attackers(ms)`: the living units that struck `unit` within the last `ms`
    /// milliseconds, rounded up to whole ticks, by stable id; none in a view with no combat.
    pub(crate) fn recent_attackers(unit: &Unit, ms: INT) -> Checked<Array> {
        let ms = u64::try_from(ms)
            .ok()
            .ok_or_else(|| ApiError::NegativeTime.fail())?;
        let view = unit.view();
        let (window, now) = (view.window(ms), view.now());
        let attackers = view.column(|column: &CombatColumn| {
            column
                .rows
                .now()
                .run(unit.row_index())
                .iter()
                // A strike later than the view's tick, as a rollback can leave, is not recent.
                .filter(|attack| now.since(attack.tick).is_some_and(|age| age <= window))
                .filter_map(|attack| view.unit(attack.source))
                .filter(|attacker| attacker.read(|row| row.alive))
                .map(Dynamic::from)
                .collect()
        });
        Ok(attackers.unwrap_or_default())
    }

    /// Fills a row of the script view with what combat holds: whether the unit lives and whether it
    /// may be a target, in the core's row; whether it stays when dead, and who struck it recently, in
    /// combat's column.
    pub(super) fn fill_row(
        parts: ROQueryItem<'_, '_, RowParts>,
        fill: &mut RowFill<'_, CombatColumn>,
    ) {
        let (dead, pools, tags, on_death, recent) = parts;
        let alive = !dead;
        fill.row.alive = alive;
        fill.row.targetable = alive
            && fill
                .column
                .life()
                .is_some_and(|life| Targets::targetable(pools, tags, life));
        let stays = on_death == Some(&OnDeath::Stay);
        fill.column
            .push(stays, recent.into_iter().flat_map(RecentAttackers::iter));
    }
}
