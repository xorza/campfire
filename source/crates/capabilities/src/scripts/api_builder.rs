use campfire_script::rhai::{Engine, RhaiNativeFunc, Variant};

use crate::scripts::hook::Hook;
use crate::scripts::script_api::{
    DataTable, HookStatus, MemberKind, MemberSpec, ScriptApi, Status,
};
use crate::stats::unit_state::UnitState;

/// Rhai's names for a property's getter and setter, and a type's indexer, which its
/// `register_get` and kin use and do not export.
const GETTER: &str = "get$";
const SETTER: &str = "set$";
const INDEX_GETTER: &str = "index$get$";
const INDEX_SETTER: &str = "index$set$";

/// Binds the script API to Rhai and records it, in one step, so no name is bound and not
/// recorded, or recorded and not bound.
#[derive(Debug)]
pub(crate) struct ApiBuilder<'a> {
    engine: &'a mut Engine,
    api: &'a mut ScriptApi,
}

impl<'a> ApiBuilder<'a> {
    pub(crate) const fn new(engine: &'a mut Engine, api: &'a mut ScriptApi) -> ApiBuilder<'a> {
        ApiBuilder { engine, api }
    }

    /// Registers `T` as scripts name its type.
    pub(crate) fn ty<T: Variant + Clone>(&mut self, name: &str) -> &mut Self {
        self.engine.register_type_with_name::<T>(name);
        self
    }

    /// Binds `f` as a form of `spec`: a value's or a field's getter, or a call, a method or an
    /// operator.
    pub(crate) fn bind<
        A: 'static,
        const N: usize,
        const X: bool,
        R: Variant + Clone,
        const F: bool,
    >(
        &mut self,
        spec: MemberSpec,
        f: impl RhaiNativeFunc<A, N, X, R, F> + 'static,
    ) -> &mut Self {
        let name = match spec.kind {
            MemberKind::Value | MemberKind::Field => format!("{GETTER}{}", spec.name),
            MemberKind::Call | MemberKind::Method | MemberKind::Operator => spec.name.to_owned(),
        };
        self.engine.register_fn(name, f);
        self.api.record(spec, false, Status::Runs);
        self
    }

    /// Binds `f` as the setter of `spec`, a field scripts may write.
    pub(crate) fn bind_set<A: 'static, const X: bool, const F: bool>(
        &mut self,
        spec: MemberSpec,
        f: impl RhaiNativeFunc<A, 2, X, (), F> + 'static,
    ) -> &mut Self {
        debug_assert_eq!(spec.kind, MemberKind::Field, "only a field is written");
        self.engine.register_fn(format!("{SETTER}{}", spec.name), f);
        self.api.record(spec, true, Status::Runs);
        self
    }

    /// Records `spec` as planned: design 08 gives it, and no code runs it yet.
    pub(crate) fn plan(&mut self, spec: MemberSpec) -> &mut Self {
        self.api.record(spec, false, Status::Planned);
        self
    }

    /// Records whether the release calls `hook`, as the code that calls it says, with its
    /// parameters' names in `signature`.
    pub(crate) fn hook(
        &mut self,
        hook: Hook,
        signature: &'static str,
        status: Status,
    ) -> &mut Self {
        self.api.record_hook(HookStatus {
            hook,
            signature,
            status,
        });
        self
    }

    /// Records whether the release honours `state`, as the code that honours it says.
    pub(crate) fn state(&mut self, state: UnitState, status: Status) -> &mut Self {
        self.api.record_state(state, status);
        self
    }

    /// Records the fields of `table` the release reads, `runs`, and those it plans.
    pub(crate) fn data(
        &mut self,
        table: DataTable,
        runs: &[&'static str],
        planned: &[&'static str],
    ) -> &mut Self {
        self.api.record_data(table, runs, planned);
        self
    }

    /// Binds `f` as the indexer of its type, through which scripts read names the data
    /// declares, as `ctx.p.<name>`: the load check reads those names against the data.
    pub(crate) fn index<A: 'static, const X: bool, R: Variant + Clone, const F: bool>(
        &mut self,
        f: impl RhaiNativeFunc<A, 2, X, R, F> + 'static,
    ) -> &mut Self {
        self.engine.register_fn(INDEX_GETTER, f);
        self
    }

    /// Binds `f` as the indexer that writes, as `ctx.state.<name> = value`.
    pub(crate) fn index_set<A: 'static, const X: bool, const F: bool>(
        &mut self,
        f: impl RhaiNativeFunc<A, 3, X, (), F> + 'static,
    ) -> &mut Self {
        self.engine.register_fn(INDEX_SETTER, f);
        self
    }
}
