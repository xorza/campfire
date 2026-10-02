use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;

use crate::scripts::call_part::CallPart;
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::ScriptRole;
use crate::stats::Stats;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::param_table::ParamTable;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::values::scalar::Scalar;

/// What stats adds to the call frame: every loaded ability's and modifier's params, which the
/// stat engine reads too; the running call's params at its rank, its ability's and its
/// modifier's, read from its acting unit; and the modifier handles it took, which write back
/// once its effects applied.
#[derive(Debug, Default)]
pub(crate) struct StatsCall {
    params: ParamBook,
    ability: Option<ActionId>,
    modifier: Option<ModifierId>,
    values: Vec<Scalar>,
    modifier_values: Vec<Scalar>,
    pub(crate) handles: Vec<ModifierHandle>,
}

impl CallPart for StatsCall {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        self.ability = start.action;
        self.modifier = start.modifier;
        self.handles.clear();
        let reads = self.ability.is_some() || self.modifier.is_some();
        let source = start
            .acting
            .filter(|_| reads)
            .and_then(|acting| ParamSource::of(world, acting));
        let rank = start.rank;
        let fill = |values: &mut Vec<Scalar>, table: &ParamTable, run: Option<usize>| {
            values.clear();
            let Some(run) = run else {
                return Ok(());
            };
            let count = table.len(run);
            values.reserve_exact(count);
            for at in 0..count {
                let value = table.value(run, at, rank, source.as_ref());
                values.push(value.ok_or(CallError::ParamOverflow)?);
            }
            Ok(())
        };
        fill(
            &mut self.values,
            self.params.actions(),
            self.ability.map(ActionId::index),
        )?;
        fill(
            &mut self.modifier_values,
            self.params.modifiers(),
            self.modifier.map(ModifierId::index),
        )
    }

    /// An ability's or a modifier's call reads its modifier's params, then its ability's.
    fn param_named(&self, role: Option<ScriptRole>, name: &str) -> Option<Dynamic> {
        if !matches!(role, Some(ScriptRole::Action | ScriptRole::Modifier)) {
            return None;
        }
        let own = self.modifier.and_then(|modifier| {
            let at = self.params.modifiers().named(modifier.index(), name)?;
            Some(self.modifier_values[at])
        });
        let value = own.or_else(|| {
            let at = self.params.actions().named(self.ability?.index(), name)?;
            Some(self.values[at])
        });
        value.map(Scalar::to_dynamic)
    }

    fn apply(&mut self, world: &mut World) {
        for handle in self.handles.drain(..) {
            Stats::write_handle(world, &handle);
        }
    }
}

impl StatsCall {
    /// Shares the params of every ability and every modifier, as the load built them, with
    /// `frame`.
    pub(crate) fn share_params(frame: &mut Frame, params: ParamBook) {
        if let Some(part) = frame.part_mut::<StatsCall>() {
            part.params = params;
        }
    }

    /// The stats' part of `frame`, to change; stats adds it as it installs.
    pub(crate) fn of_mut(frame: &mut Frame) -> &mut StatsCall {
        frame
            .part_mut()
            .expect("a match whose calls take modifier handles has stats")
    }

    /// The running call's ability's param at `at`, at its rank.
    pub(crate) fn ability_value(frame: &Frame, at: usize) -> Scalar {
        let part = frame
            .part::<StatsCall>()
            .expect("a call that reads an ability's params runs in a match with stats");
        part.values[at]
    }
}
