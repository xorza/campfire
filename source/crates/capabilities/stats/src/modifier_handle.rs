use std::cell::{RefCell, RefMut};
use std::rc::Rc;
use std::sync::Arc;

use campfire_script::{
    ApiBuilder, ApiError, ApiHolder, CapabilityRefusal, Checked, MemberDecl, ScriptHandle,
    ScriptValue, StateField, StateSchema, StateValue, View, plain_script_arg,
};
use campfire_sim::StableId;
use campfire_values::{ModifierId, NameId};

use crate::error::StatsRefusal;

/// A modifier as a script holds it, `Modifier` in scripts: its carrier and source, and its
/// stacks and state, which a call may write and read back; the writes apply when the call ends.
#[derive(Debug, Clone)]
pub struct ModifierHandle(Rc<RefCell<HandleData>>);

/// What a handle holds: which instance, its stacks and state as the call sees them, whether
/// the call wrote either, and whether the call removed the instance.
#[derive(Debug)]
pub struct HandleData {
    pub(crate) carrier: StableId,
    pub id: ModifierId,
    pub(crate) source: Option<StableId>,
    pub stacks: u32,
    pub state: Vec<StateValue>,
    pub written: bool,
    pub removed: bool,
    schema: Arc<StateSchema>,
    view: View,
}

/// `m.state`: the modifier's state fields, by name, to read and write.
#[derive(Debug, Clone)]
pub(crate) struct ModifierState(ModifierHandle);

/// The instance a handle is to, of `id` from `source` on `carrier`, and its stacks as the call
/// starts to see them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandleOf {
    pub carrier: StableId,
    pub id: ModifierId,
    pub source: Option<StableId>,
    pub stacks: u32,
}

/// What a new handle is made of: the instance it is to, its state's `schema`, and the view the
/// call reads.
#[derive(Debug, Clone)]
pub struct HandleSetup {
    pub of: HandleOf,
    pub schema: Arc<StateSchema>,
    pub view: View,
}

impl ModifierHandle {
    /// A handle of `setup`, with `state` as the call starts to see it: `spare`, a handle no
    /// script holds any more, filled again when given, so its buffers serve once more.
    pub fn new<'a>(
        spare: Option<Self>,
        setup: HandleSetup,
        state: impl IntoIterator<Item = &'a StateValue>,
    ) -> Self {
        Self::filled(spare, setup, &mut state.into_iter())
    }

    fn filled(
        spare: Option<Self>,
        setup: HandleSetup,
        state: &mut dyn Iterator<Item = &StateValue>,
    ) -> Self {
        let HandleSetup { of, schema, view } = setup;
        let HandleOf {
            carrier,
            id,
            source,
            stacks,
        } = of;
        if let Some(mut handle) = spare {
            let held = Rc::get_mut(&mut handle.0).expect("no script holds a spare handle");
            let data = held.get_mut();
            data.carrier = carrier;
            data.id = id;
            data.source = source;
            data.stacks = stacks;
            data.state.clear();
            data.state.extend(state.cloned());
            data.written = false;
            data.removed = false;
            data.schema = schema;
            data.view = view;
            return handle;
        }
        Self(Rc::new(RefCell::new(HandleData {
            carrier,
            id,
            source,
            stacks,
            state: state.cloned().collect(),
            written: false,
            removed: false,
            schema,
            view,
        })))
    }

    /// Whether only the call that made it holds it: no script keeps a copy.
    pub(crate) fn is_unshared(&self) -> bool {
        Rc::strong_count(&self.0) == 1
    }

    /// What it holds, borrowed until the guard drops.
    pub fn data(&self) -> RefMut<'_, HandleData> {
        self.0.borrow_mut()
    }

    /// Whether it is the handle of the instance of `id` from `source` on `carrier`.
    pub(crate) fn is(&self, carrier: StableId, id: ModifierId, source: Option<StableId>) -> bool {
        let data = self.0.borrow();
        (data.carrier, data.id, data.source) == (carrier, id, source)
    }

    /// The `Modifier` handle's fields, and `m.state`.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field =
            |name, description| MemberDecl::field(ApiHolder::of::<Self>(), name, description);
        let stacks = field("stacks", "its stacks, which a call may write and read back");
        api.ty::<Self>()
            .bind(
                field("carrier", "the unit that carries it"),
                |handle: &mut Self| {
                    let data = handle.data();
                    data.view.unit_value(Some(data.carrier))
                },
            )
            .bind(
                field("source", "the unit that applied it, `()` when gone or none"),
                |handle: &mut Self| {
                    let data = handle.data();
                    data.view.unit_value(data.source)
                },
            )
            .bind(stacks, |handle: &mut Self| i64::from(handle.data().stacks))
            .bind_set(stacks, |handle: &mut Self, stacks: i64| -> Checked<()> {
                let stacks = u32::try_from(stacks)
                    .ok()
                    .ok_or_else(|| StatsRefusal::NegativeStacks.fail())?;
                let mut data = handle.data();
                data.stacks = stacks;
                data.written = true;
                Ok(())
            })
            .bind(
                field(
                    "state",
                    "its script state, by name, which a call may write and read back",
                ),
                |handle: &mut Self| ModifierState(handle.clone()),
            );
        api.ty::<ModifierState>()
            .index(|_, state: &mut ModifierState, name: &str| {
                let data = state.0.data();
                let field = data.field(name)?;
                Ok(data
                    .schema
                    .read(field, &data.state[field.index()], &data.view))
            })
            .index_set(
                |_, state: &mut ModifierState, name: &str, value: ScriptValue| -> Checked<()> {
                    let mut data = state.0.data();
                    let field = data.field(name)?;
                    let value = data.schema.take(field, &value)?;
                    data.state[field.index()] = value;
                    data.written = true;
                    Ok(())
                },
            );
    }
}

impl HandleData {
    /// The state field `name`; one the modifier does not declare fails the call.
    fn field(&self, name: &str) -> Checked<StateField> {
        let field = self.schema.field(name);
        field.ok_or_else(|| ApiError::UnknownState.fail().into())
    }
}

plain_script_arg!(ModifierHandle);

impl ScriptHandle for ModifierHandle {
    const NAME: &'static str = "Modifier";
    const TITLE: &'static str = "Modifier `m`";
}

impl ScriptHandle for ModifierState {
    const NAME: &'static str = "ModifierState";
    const TITLE: &'static str = "A modifier's state";
}
