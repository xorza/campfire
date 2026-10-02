use campfire_script::rhai::{Array, Dynamic, INT};

use crate::combat::recent_attack::RecentAttack;
use crate::scripts::error::{ApiError, Checked};
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

/// What combat adds to the script view: whether each unit stays when it dies, and the recent
/// attacks on it, a run for each row, in one buffer.
#[derive(Debug, Default)]
pub(crate) struct CombatColumn {
    stays: Vec<bool>,
    attacks: Vec<RecentAttack>,
    /// Where each row's run starts.
    starts: Vec<u32>,
}

impl ViewColumn for CombatColumn {
    fn clear(&mut self) {
        self.stays.clear();
        self.attacks.clear();
        self.starts.clear();
    }

    fn rows(&self) -> usize {
        self.starts.len()
    }
}

impl CombatColumn {
    /// Adds the row of a unit that stays when it dies when `stays`, and that `attacks` struck
    /// recently.
    pub(crate) fn push(&mut self, stays: bool, attacks: impl IntoIterator<Item = RecentAttack>) {
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

    /// Whether `unit` stays when it dies, for the mode to bring back; none does in a view with no
    /// combat.
    pub(crate) fn stays(unit: &Unit) -> bool {
        let view = unit.view();
        let stays = view.column(|column: &CombatColumn| column.stays[unit.row_index()]);
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
