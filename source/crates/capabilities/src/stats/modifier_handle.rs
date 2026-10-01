use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use campfire_script::rhai::{Dynamic, INT, ImmutableString};
use campfire_sim::{Capability, StableId};

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_api::{ApiOwner, MemberSpec};
use crate::scripts::state_decl::StateType;
use crate::scripts::state_value::StateValue;
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifier_effect::ModifierEffect;
use crate::units::script_view::View;

/// A modifier as a script holds it, `Modifier` in scripts: its carrier and source, and its
/// stacks and state, which a call may write and read back; the writes apply when the call ends.
#[derive(Debug, Clone)]
pub(crate) struct ModifierHandle(Rc<RefCell<HandleData>>);

/// What a handle holds: which instance, its stacks and state as the call sees them, whether
/// the call wrote either, and whether the call removed the instance.
#[derive(Debug)]
pub(crate) struct HandleData {
    pub(crate) carrier: StableId,
    pub(crate) id: ModifierId,
    pub(crate) source: Option<StableId>,
    pub(crate) stacks: u32,
    pub(crate) state: Vec<StateValue>,
    pub(crate) written: bool,
    pub(crate) removed: bool,
    fields: Rc<[StateField]>,
    view: View,
}

/// A field of a modifier's script state: its name and type, in the order of the names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StateField {
    pub(crate) name: Box<str>,
    pub(crate) kind: StateType,
}

/// `m.state`: the modifier's state fields, by name, to read and write.
#[derive(Debug, Clone)]
pub(crate) struct ModifierState(ModifierHandle);

impl ModifierHandle {
    /// A handle to the instance of `id` from `source` on `carrier`, with `stacks` and `state` as
    /// the call starts to see them, and its state's `fields`.
    pub(crate) fn new(
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
        stacks: u32,
        state: Vec<StateValue>,
        fields: Rc<[StateField]>,
        view: View,
    ) -> ModifierHandle {
        ModifierHandle(Rc::new(RefCell::new(HandleData {
            carrier,
            id,
            source,
            stacks,
            state,
            written: false,
            removed: false,
            fields,
            view,
        })))
    }

    /// What it holds, borrowed until the guard drops.
    pub(crate) fn data(&self) -> RefMut<'_, HandleData> {
        self.0.borrow_mut()
    }

    /// Whether it is the handle of the instance of `id` from `source` on `carrier`.
    pub(crate) fn is(&self, carrier: StableId, id: ModifierId, source: Option<StableId>) -> bool {
        let data = self.0.borrow();
        (data.carrier, data.id, data.source) == (carrier, id, source)
    }

    /// `ctx.remove(m)`: the effect that ends its instance, which the call now sees as gone.
    pub(crate) fn remove(&self) -> ModifierEffect {
        let mut data = self.data();
        data.removed = true;
        ModifierEffect::Remove {
            carrier: data.carrier,
            id: data.id,
            source: data.source,
        }
    }

    /// The `Modifier` handle's fields, and `m.state`.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field = |name, description| {
            MemberSpec::field(ApiOwner::Modifier, name, description).capability(Capability::Stats)
        };
        let stacks = field("stacks", "its stacks, which a call may write and read back");
        api.ty::<ModifierHandle>("Modifier")
            .bind(
                field("carrier", "the unit that carries it"),
                |m: &mut ModifierHandle| {
                    let data = m.data();
                    data.view
                        .unit(data.carrier)
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            )
            .bind(
                field("source", "the unit that applied it, `()` when gone or none"),
                |m: &mut ModifierHandle| {
                    let data = m.data();
                    data.source
                        .and_then(|source| data.view.unit(source))
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            )
            .bind(stacks, |m: &mut ModifierHandle| INT::from(m.data().stacks))
            .bind_set(
                stacks,
                |m: &mut ModifierHandle, stacks: INT| -> Checked<()> {
                    let stacks = u32::try_from(stacks)
                        .ok()
                        .ok_or_else(|| ApiError::NegativeStacks.fail())?;
                    let mut data = m.data();
                    data.stacks = stacks;
                    data.written = true;
                    Ok(())
                },
            )
            .bind(
                field(
                    "state",
                    "its script state, by name, which a call may write and read back",
                ),
                |m: &mut ModifierHandle| ModifierState(m.clone()),
            );
        api.ty::<ModifierState>("ModifierState")
            .index(
                |state: &mut ModifierState, name: ImmutableString| -> Checked<Dynamic> {
                    let data = state.0.data();
                    let at = data.field(&name)?;
                    Ok(data.state[at].to_dynamic(&data.view))
                },
            )
            .index_set(
                |state: &mut ModifierState, name: ImmutableString, value: Dynamic| -> Checked<()> {
                    let mut data = state.0.data();
                    let at = data.field(&name)?;
                    let value = StateValue::from_dynamic(data.fields[at].kind, &value)
                        .ok_or_else(|| ApiError::WrongStateType.fail())?;
                    data.state[at] = value;
                    data.written = true;
                    Ok(())
                },
            );
    }
}

impl HandleData {
    /// The place of the state field `name`; one the modifier does not declare fails the call.
    fn field(&self, name: &str) -> Checked<usize> {
        self.fields
            .binary_search_by(|field| (*field.name).cmp(name))
            .ok()
            .ok_or_else(|| ApiError::UnknownState.fail().into())
    }
}
