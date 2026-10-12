use campfire_script::ScriptHost;
use campfire_script::rhai::{
    Dynamic, ImmutableString, Module, NativeCallContext, RhaiNativeFunc, Variant,
};
use campfire_sim::Capability;

use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx_fn::{CtxFn, MethodFn};
use crate::scripts::data_section::DataSection;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::ScriptApi;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::enum_record::EnumRecord;
use crate::scripts::script_api::hook_status::HookStatus;
use crate::scripts::script_api::member_kind::MemberKind;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::units::tag_property::TagProperty;
use crate::values::script_enum::ScriptEnum;

/// Rhai's names for a property's getter and setter, and a type's indexer, which its
/// `register_get` and kin use and do not export.
const GETTER: &str = "get$";
const SETTER: &str = "set$";
const INDEX_GETTER: &str = "index$get$";
const INDEX_SETTER: &str = "index$set$";

/// Binds the script API to Rhai and records it, in one step, so no name is bound and not
/// recorded, or recorded and not bound. Each name it binds is of the capability of the module that
/// binds it, unless its spec names another.
#[derive(Debug)]
pub(crate) struct ApiBuilder<'a> {
    host: &'a mut ScriptHost,
    api: &'a mut ScriptApi,
    /// The capability whose module binds, none for the core's and the mode's.
    capability: Option<Capability>,
}

impl<'a> ApiBuilder<'a> {
    pub(crate) const fn new(host: &'a mut ScriptHost, api: &'a mut ScriptApi) -> ApiBuilder<'a> {
        ApiBuilder {
            host,
            api,
            capability: None,
        }
    }

    /// Starts the API of the module of `capability`: the core's, `None`, or a capability's. The
    /// mode's names are the core's, as every match has a mode.
    pub(crate) fn module(&mut self, capability: Option<Capability>) {
        self.capability = capability.filter(|&capability| capability != Capability::Mode);
    }

    /// `spec`, of the binding module's capability unless it names another.
    fn owned(&self, mut spec: MemberSpec) -> MemberSpec {
        assert!(
            self.capability.is_none() || spec.capability != self.capability,
            "`{}` names the capability of the module that binds it",
            spec.name
        );
        spec.capability = spec.capability.or(self.capability);
        spec
    }

    /// Registers `T` as scripts name its type.
    pub(crate) fn ty<T: Variant + Clone>(&mut self, name: &str) -> &mut Self {
        self.host.engine_mut().register_type_with_name::<T>(name);
        self
    }

    /// Binds the engine enum of `T`: its type, under the enum's name; a module of that name
    /// with a constant for each member, and `named`; and `==`, `!=` and `to_string` on its
    /// members.
    pub(crate) fn engine_enum<T: ScriptEnum>(&mut self) -> &mut Self {
        let name = T::ENUM.name();
        let [eq, ne, to_string] = EnumRecord::MEMBER_FUNCTIONS;
        self.host
            .engine_mut()
            .register_type_with_name::<T>(name)
            .register_fn(eq, |a: T, b: T| a == b)
            .register_fn(ne, |a: T, b: T| a != b)
            .register_fn(to_string, |member: &mut T| {
                ImmutableString::from(member.data_name())
            });
        let mut module = Module::new();
        for &(member, value) in T::MEMBERS {
            module.set_var(member, value);
        }
        let [named] = EnumRecord::FUNCTIONS;
        module.set_native_fn(named, |text: ImmutableString| -> Checked<T> {
            T::named(&text).ok_or_else(|| ApiError::UnknownMember(T::ENUM).fail().into())
        });
        self.host
            .engine_mut()
            .register_static_module(name, module.into());
        let members = T::MEMBERS.iter().map(|&(member, _)| member).collect();
        self.api.record_enum(T::ENUM, members);
        self
    }

    /// Binds `f` as the form of `spec`, a value or a field's getter, which serves every role and
    /// takes no argument.
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
        let spec = self.owned(spec);
        assert!(
            matches!(spec.kind, MemberKind::Value | MemberKind::Field),
            "`{}` takes arguments, which `call` or `method` binds",
            spec.name
        );
        assert_eq!(
            spec.roles,
            RoleSet::ALL,
            "`{}` serves some roles only, which `call` checks",
            spec.name
        );
        self.host
            .engine_mut()
            .register_fn(ApiBuilder::bound_name(&spec), f);
        self.api
            .record(spec, false, Status::Runs(ApiVersion::FIRST));
        self
    }

    /// Binds `f`, a form of `spec`, a call or a value of `ctx` that takes `ctx` first, for the
    /// roles of `spec`: a call in another role fails before `f` runs. The types of its arguments
    /// say what each names.
    pub(crate) fn call<Args, R, F: CtxFn<Args, R>>(&mut self, spec: MemberSpec, f: F) -> &mut Self {
        let spec = self.owned(spec).with_args(F::args());
        assert!(
            matches!(spec.kind, MemberKind::Call | MemberKind::Value),
            "`{}` is no call or value of `ctx`",
            spec.name
        );
        f.register(
            self.host.engine_mut(),
            &ApiBuilder::bound_name(&spec),
            spec.roles,
        );
        self.api
            .record(spec, false, Status::Runs(ApiVersion::FIRST));
        self
    }

    /// Binds `f`, a form of `spec`, a method or an operator of a handle, which serves every role.
    /// The types of its arguments say what each names.
    pub(crate) fn method<M, H, Args, R, F: MethodFn<M, H, Args, R>>(
        &mut self,
        spec: MemberSpec,
        f: F,
    ) -> &mut Self {
        let spec = self.owned(spec).with_args(F::args());
        assert!(
            matches!(spec.kind, MemberKind::Method | MemberKind::Operator),
            "`{}` is no method or operator",
            spec.name
        );
        assert_eq!(
            spec.roles,
            RoleSet::ALL,
            "`{}`, a method, serves every role",
            spec.name
        );
        f.register(self.host.engine_mut(), &ApiBuilder::bound_name(&spec));
        self.api
            .record(spec, false, Status::Runs(ApiVersion::FIRST));
        self
    }

    /// The name Rhai calls `spec` by: its getter's, for a value or a field.
    fn bound_name(spec: &MemberSpec) -> String {
        match spec.kind {
            MemberKind::Value | MemberKind::Field => format!("{GETTER}{}", spec.name),
            MemberKind::Call | MemberKind::Method | MemberKind::Operator => spec.name.to_owned(),
        }
    }

    /// Binds `f` as the setter of `spec`, a field scripts may write.
    pub(crate) fn bind_set<A: 'static, const X: bool, const F: bool>(
        &mut self,
        spec: MemberSpec,
        f: impl RhaiNativeFunc<A, 2, X, (), F> + 'static,
    ) -> &mut Self {
        let spec = self.owned(spec);
        debug_assert_eq!(spec.kind, MemberKind::Field, "only a field is written");
        self.host
            .engine_mut()
            .register_fn(format!("{SETTER}{}", spec.name), f);
        self.api.record(spec, true, Status::Runs(ApiVersion::FIRST));
        self
    }

    /// Records `spec` as planned: design 08 gives it, and no code runs it yet.
    pub(crate) fn plan(&mut self, spec: MemberSpec) -> &mut Self {
        let spec = self.owned(spec);
        self.api.record(spec, false, Status::Planned);
        self
    }

    /// Records whether the release calls `hook`, as the code that calls it says, of the
    /// capability of the module that calls it and binds.
    pub(crate) fn hook(&mut self, hook: Hook, status: Status) -> &mut Self {
        self.api.record_hook(HookStatus {
            hook,
            status,
            capability: self.capability,
        });
        self
    }

    /// Records whether the release honours `property`, as the code that honours it says.
    pub(crate) fn tag_property(&mut self, property: TagProperty, status: Status) -> &mut Self {
        self.api.record_tag_property(property, status);
        self
    }

    /// Records the fields of `T`'s table the release reads, `runs`, and those it plans.
    pub(crate) fn data<T: DataSection>(
        &mut self,
        runs: &[&'static str],
        planned: &[&'static str],
    ) -> &mut Self {
        self.api
            .record_data(T::TABLE, runs, planned, self.capability);
        self
    }

    /// Records the fields of `table`, the mode's or a unit type's, which many modules share, that
    /// the release reads, `runs`, and those it plans: the module's own.
    pub(crate) fn shared_data(
        &mut self,
        table: DataTable,
        runs: &[&'static str],
        planned: &[&'static str],
    ) -> &mut Self {
        assert!(
            matches!(table, DataTable::Mode | DataTable::UnitType),
            "{table:?} is one module's, which `data` records by its type"
        );
        self.api.record_data(table, runs, planned, self.capability);
        self
    }

    /// Records `fields` of an action's data, each by its name with its status.
    pub(crate) fn action_fields(
        &mut self,
        fields: impl IntoIterator<Item = (&'static str, Status)>,
    ) -> &mut Self {
        for (name, status) in fields {
            self.api
                .record_field(DataTable::Action, name, status, self.capability);
        }
        self
    }

    /// Binds `get` as the indexer of `T`, through which scripts read names the data declares, as
    /// `ctx.p.<name>`, and as the getter of each such name scripts use: the load check reads those
    /// names against the data. `get` has the running call's context, for a `T` that reads it.
    pub(crate) fn index<T: Variant + Clone>(
        &mut self,
        get: fn(NativeCallContext<'_>, &mut T, &str) -> Checked<Dynamic>,
    ) -> &mut Self {
        self.host.engine_mut().register_fn(
            INDEX_GETTER,
            move |call: NativeCallContext<'_>, target: &mut T, name: ImmutableString| {
                get(call, target, &name)
            },
        );
        self.host.forward_properties(move |engine, name| {
            let property = ImmutableString::from(name);
            engine.register_fn(
                format!("{GETTER}{name}"),
                move |call: NativeCallContext<'_>, target: &mut T| get(call, target, &property),
            );
        });
        self
    }

    /// Binds `set` as the indexer of `T` that writes, as `ctx.state.<name> = value`, and as the
    /// setter of each such name scripts use. `set` has the running call's context, as `index`'s
    /// getter does.
    pub(crate) fn index_set<T: Variant + Clone>(
        &mut self,
        set: fn(NativeCallContext<'_>, &mut T, &str, Dynamic) -> Checked<()>,
    ) -> &mut Self {
        self.host.engine_mut().register_fn(
            INDEX_SETTER,
            move |call: NativeCallContext<'_>,
                  target: &mut T,
                  name: ImmutableString,
                  value: Dynamic| { set(call, target, &name, value) },
        );
        self.host.forward_properties(move |engine, name| {
            let property = ImmutableString::from(name);
            engine.register_fn(
                format!("{SETTER}{name}"),
                move |call: NativeCallContext<'_>, target: &mut T, value: Dynamic| {
                    set(call, target, &property, value)
                },
            );
        });
        self
    }
}
