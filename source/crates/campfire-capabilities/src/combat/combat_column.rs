use bevy_ecs::world::World;
use campfire_script::rhai::{Array, Dynamic, INT};

use crate::combat::recent_attack::RecentAttack;
use crate::scripts::error::{ApiError, Checked};
use crate::stats::life_pool::LifePool;
use crate::stats::pool_id::PoolId;
use crate::units::kept_rows::{ColumnRows, KeptRows};
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

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

    fn keep(&mut self, row: usize) {
        self.rows.keep(row);
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

    fn push_from(&mut self, from: &Self, row: usize) {
        self.push(from.stays[row], from.run(row).iter().copied());
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
        let end = self
            .starts
            .get(row + 1)
            .map_or(self.attacks.len(), |&end| end as usize);
        &self.attacks[self.starts[row] as usize..end]
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
                .filter(|attacker| attacker.row().alive)
                .map(Dynamic::from)
                .collect()
        });
        Ok(attackers.unwrap_or_default())
    }
}
