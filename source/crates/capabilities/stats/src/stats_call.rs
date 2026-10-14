use bevy_ecs::world::World;
use campfire_script::{CallError, CallPart, CallStart, Frame, ScriptRole, ScriptValue};
use campfire_values::{ActionId, ModifierId, Scalar};

use crate::Stats;
use crate::modifier_handle::ModifierHandle;
use crate::param_book::ParamBook;
use crate::param_source::ParamSource;
use crate::param_table::ParamTable;

/// What stats adds to the call frame: every loaded ability's and modifier's params, which the
/// stat engine reads too; the running call's params at its rank, its ability's and its
/// modifier's, read from its acting unit; and the modifier handles it took, which write back
/// once its effects applied.
#[derive(Debug, Default)]
pub struct StatsCall {
    params: ParamBook,
    action: Option<ActionId>,
    modifier: Option<ModifierId>,
    values: Vec<Scalar>,
    modifier_values: Vec<Scalar>,
    pub handles: Vec<ModifierHandle>,
    /// The handles of earlier calls that no script holds, for new handles to fill again.
    spare: Vec<ModifierHandle>,
}

impl CallPart for StatsCall {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        let params = world.resource::<ParamBook>();
        if !params.same(&self.params) {
            self.params = params.clone();
        }
        self.action = start.action;
        self.modifier = start.modifier;
        let unshared = self.handles.drain(..).filter(ModifierHandle::unshared);
        self.spare.extend(unshared);
        let reads = self.action.is_some() || self.modifier.is_some();
        let source = start
            .acting
            .filter(|_| reads)
            .and_then(|acting| ParamSource::of(world, acting));
        let rank = start.rank;
        let fill = |values: &mut Vec<Scalar>, table: &ParamTable, run: Option<usize>| {
            values.clear();
            let Some(run) = run else {
                return;
            };
            let count = table.len(run);
            values.reserve_exact(count);
            values.extend((0..count).map(|at| table.value(run, at, rank, source.as_ref())));
        };
        fill(
            &mut self.values,
            self.params.actions(),
            self.action.map(ActionId::index),
        );
        fill(
            &mut self.modifier_values,
            self.params.modifiers(),
            self.modifier.map(ModifierId::index),
        );
        Ok(())
    }

    /// An ability's or a modifier's call reads its modifier's params, then its ability's.
    fn param(&self, role: Option<ScriptRole>, name: &str) -> Option<ScriptValue> {
        if !matches!(role, Some(ScriptRole::Action | ScriptRole::Modifier)) {
            return None;
        }
        let own = self.modifier.and_then(|modifier| {
            let at = self.params.modifiers().named(modifier.index(), name)?;
            Some(self.modifier_values[at])
        });
        let value = own.or_else(|| {
            let at = self.params.actions().named(self.action?.index(), name)?;
            Some(self.values[at])
        });
        value.map(ScriptValue::from)
    }

    fn apply(&mut self, world: &mut World) {
        for handle in self.handles.drain(..) {
            Stats::write_handle(world, &handle);
        }
    }
}

impl StatsCall {
    /// A handle of an earlier call that no script holds, to fill again.
    pub fn spare(&mut self) -> Option<ModifierHandle> {
        self.spare.pop()
    }

    /// The stats' part of `frame`; stats adds it as it installs.
    pub fn of(frame: &Frame) -> &StatsCall {
        frame
            .part()
            .expect("a match whose calls read params or apply modifiers has stats")
    }

    /// Every loaded ability's and modifier's params.
    pub(crate) const fn params(&self) -> &ParamBook {
        &self.params
    }

    /// The stats' part of `frame`, to change; stats adds it as it installs.
    pub fn of_mut(frame: &mut Frame) -> &mut StatsCall {
        frame
            .part_mut()
            .expect("a match whose calls take modifier handles has stats")
    }

    /// The running call's ability's param at `at`, at its rank.
    pub fn ability_value(frame: &Frame, at: usize) -> Scalar {
        StatsCall::of(frame).values[at]
    }
}
