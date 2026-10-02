use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;

use crate::scripts::call_part::CallPart;
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::script_role::ScriptRole;
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
    /// The handles of earlier calls that no script holds, for new handles to fill again.
    spare: Vec<ModifierHandle>,
}

impl CallPart for StatsCall {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        self.ability = start.action;
        self.modifier = start.modifier;
        let unshared = self.handles.drain(..).filter(ModifierHandle::unshared);
        self.spare.extend(unshared);
        let reads = self.ability.is_some() || self.modifier.is_some();
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
            self.ability.map(ActionId::index),
        );
        fill(
            &mut self.modifier_values,
            self.params.modifiers(),
            self.modifier.map(ModifierId::index),
        );
        Ok(())
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
    /// A handle of an earlier call that no script holds, to fill again.
    pub(crate) fn spare(&mut self) -> Option<ModifierHandle> {
        self.spare.pop()
    }

    /// Shares the params of every ability and every modifier, as the load built them, with
    /// `frame`.
    pub(crate) fn share_params(frame: &mut Frame, params: ParamBook) {
        if let Some(part) = frame.part_mut::<StatsCall>() {
            part.params = params;
        }
    }

    /// The stats' part of `frame`; stats adds it as it installs.
    pub(crate) fn of(frame: &Frame) -> &StatsCall {
        frame
            .part()
            .expect("a match whose calls read params or apply modifiers has stats")
    }

    /// Every loaded ability's and modifier's params.
    pub(crate) const fn params(&self) -> &ParamBook {
        &self.params
    }

    /// The stats' part of `frame`, to change; stats adds it as it installs.
    pub(crate) fn of_mut(frame: &mut Frame) -> &mut StatsCall {
        frame
            .part_mut()
            .expect("a match whose calls take modifier handles has stats")
    }

    /// The running call's ability's param at `at`, at its rank.
    pub(crate) fn ability_value(frame: &Frame, at: usize) -> Scalar {
        StatsCall::of(frame).values[at]
    }
}

#[cfg(test)]
mod tests {

    use std::ptr;
    use std::sync::Arc;

    use campfire_sim::IdAllocator;

    use super::*;
    use crate::capability_set::test_match::TestMatch;
    use crate::scripts::state_value::StateValue;
    use crate::stats::modifier_handle::HandleOf;
    use crate::units::script_view::View;

    #[test]
    fn a_handle_no_script_holds_serves_the_next_call_again() {
        let view = View::new(TestMatch::RATE);
        let carrier = IdAllocator::default().allocate();
        let of = |stacks| HandleOf {
            carrier,
            id: ModifierId::new(0),
            source: None,
            stacks,
        };
        let handle = |stacks, state: &[StateValue]| {
            ModifierHandle::new(None, of(stacks), state, Arc::from([]), view.clone())
        };
        // The call holds two handles; a script still holds a copy of the second as the next
        // call begins, so only the first is spare.
        let mut call = StatsCall::default();
        call.handles.push(handle(1, &[StateValue::Int(1)]));
        call.handles.push(handle(2, &[]));
        let held = call.handles[1].clone();
        let first = ptr::from_ref(&*call.handles[0].data()).addr();
        call.begin(&World::new(), &CallStart::mode(ScriptRole::Mode))
            .unwrap();
        assert!(call.handles.is_empty());
        let spare = call.spare().unwrap();
        assert!(call.spare().is_none());
        // Filled again, it is the first's storage with the new instance, and nothing written.
        let state = [StateValue::Int(7), StateValue::Int(8)];
        let again = ModifierHandle::new(Some(spare), of(3), &state, Arc::from([]), view.clone());
        let data = again.data();
        assert_eq!(ptr::from_ref(&*data).addr(), first);
        assert_eq!((data.stacks, data.state.as_slice()), (3, &state[..]));
        assert!(!data.written && !data.removed);
        assert_eq!(held.data().stacks, 2);
    }
}
