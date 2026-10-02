use campfire_script::rhai::{Array, Dynamic, INT};

use crate::combat::recent_attack::RecentAttack;
use crate::scripts::error::{ApiError, Checked};
use crate::units::unit::Unit;
use crate::units::view_column::ViewColumn;

/// What combat adds to the script view: the recent attacks on each unit, a run for each row, in
/// one buffer.
#[derive(Debug, Default)]
pub(crate) struct AttacksColumn {
    attacks: Vec<RecentAttack>,
    /// Where each row's run starts.
    starts: Vec<u32>,
}

impl ViewColumn for AttacksColumn {
    fn clear(&mut self) {
        self.attacks.clear();
        self.starts.clear();
    }

    fn rows(&self) -> usize {
        self.starts.len()
    }
}

impl AttacksColumn {
    /// Adds the row of a unit `attacks` struck recently.
    pub(crate) fn push(&mut self, attacks: impl IntoIterator<Item = RecentAttack>) {
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

    /// `unit.recent_attackers(ms)`: the living units that struck `unit` within the last `ms`
    /// milliseconds, rounded up to whole ticks, by stable id; none in a view with no combat.
    pub(crate) fn recent_attackers(unit: &Unit, ms: INT) -> Checked<Array> {
        let ms = u64::try_from(ms)
            .ok()
            .ok_or_else(|| ApiError::NegativeTime.fail())?;
        let view = unit.view();
        let (window, now) = (view.window(ms), view.now());
        let attackers = view.column(|column: &AttacksColumn| {
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
