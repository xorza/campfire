use std::any::TypeId;

use campfire_sim::Capability;
use campfire_values::{ScriptEnum, TagProperty};
use rhai::{Dynamic, ImmutableString, Module, NativeCallContext};

use crate::script_host::ScriptHost;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::ctx_fn::{CtxFn, GetFn, MethodFn};
use crate::scripts::data_section::DataSection;
use crate::scripts::data_table::DataTable;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::member_kind::{GETTER, MemberKind};
use crate::scripts::member_spec::MemberSpec;
use crate::scripts::planned_capability::PlannedCapability;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::ScriptApi;
use crate::scripts::script_api::api_member::ApiMember;
use crate::scripts::script_api::data_field::DataField;
use crate::scripts::script_api::enum_record::EnumRecord;
use crate::scripts::script_api::hook_status::HookStatus;
use crate::scripts::script_arg::ScriptArg;
use crate::scripts::script_value::{ScriptType, ScriptValue};
use crate::scripts::status::Status;

/// Rhai's names for a property's setter, and a type's indexer, which its `register_get` and kin
/// use and do not export.
const SETTER: &str = "set$";
const INDEX_GETTER: &str = "index$get$";
const INDEX_SETTER: &str = "index$set$";

/// Binds the script API to Rhai and records it, in one step, so no name is bound and not
/// recorded, or recorded and not bound. Each name it binds is of the capability of the module that
/// binds it, unless its spec names another.
#[derive(Debug)]
pub struct ApiBuilder<'a> {
    host: &'a mut ScriptHost,
    api: &'a mut ScriptApi,
    /// The capability whose module binds, none for the core's and the mode's.
    capability: Option<Capability>,
}

impl<'a> ApiBuilder<'a> {
    pub(crate) const fn new(host: &'a mut ScriptHost, api: &'a mut ScriptApi) -> Self {
        ApiBuilder {
            host,
            api,
            capability: None,
        }
    }

    /// Starts the API of the module of `capability`: the core's, `None`, or a capability's. The
    /// mode's names are the core's, as every match has a mode.
    pub fn module(&mut self, capability: Option<Capability>) {
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
    pub fn ty<T: ScriptType>(&mut self, name: &str) -> &mut Self {
        self.host.engine_mut().register_type_with_name::<T>(name);
        self
    }

    /// Binds the engine enum of `T`: its type, under the enum's name; a module of that name
    /// with a constant for each member, and `named`; and `==`, `!=` and `to_string` on its
    /// members.
    pub fn engine_enum<T: ScriptEnum + ScriptType>(&mut self) -> &mut Self {
        let name = T::ENUM.name();
        let [eq, ne, to_string] = EnumRecord::MEMBER_FUNCTIONS;
        self.host
            .engine_mut()
            .register_type_with_name::<T>(name)
            .register_fn(eq, |a: T, b: T| a == b)
            .register_fn(ne, |a: T, b: T| a != b)
            .register_fn(to_string, |member: &mut T| {
                ImmutableString::from(member.name())
            });
        let mut module = Module::new();
        for (member, value) in T::members() {
            module.set_var(member, value);
        }
        let [named] = EnumRecord::FUNCTIONS;
        module.set_native_fn(named, |text: ImmutableString| -> Checked<T> {
            T::named(&text).ok_or_else(|| ApiError::UnknownMember(T::ENUM).fail().into())
        });
        self.host
            .engine_mut()
            .register_static_module(name, module.into());
        let members = T::members().map(|(member, _)| member);
        self.api.record_enum(T::ENUM, members);
        self
    }

    /// Binds `f` as the form of `spec`, a value or a field's getter, which serves every role and
    /// takes no argument.
    pub fn bind<M, H, O, K, F: GetFn<M, H, O, K>>(&mut self, spec: MemberSpec, f: F) -> &mut Self {
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
        f.register(self.host.engine_mut(), &spec.kind.bound_name(spec.name));
        self.record_runs(spec, F::output());
        self
    }

    /// Binds `f`, a form of `spec`, a call or a value of `ctx` that takes `ctx` first, for the
    /// roles of `spec`: a call in another role fails before `f` runs. The types of its arguments
    /// say what each names.
    pub fn call<Args, R, F: CtxFn<Args, R>>(&mut self, spec: MemberSpec, f: F) -> &mut Self {
        let spec = self.owned(spec).with_args(F::args());
        assert!(
            matches!(spec.kind, MemberKind::Call | MemberKind::Value),
            "`{}` is no call or value of `ctx`",
            spec.name
        );
        assert!(
            spec.has_form_of(F::ARITY),
            "a form of `{}` takes the {} arguments its function takes",
            spec.name,
            F::ARITY
        );
        f.register(
            self.host.engine_mut(),
            &spec.kind.bound_name(spec.name),
            spec.roles,
        );
        self.record_runs(spec, F::output());
        self
    }

    /// Binds `f`, a form of `spec`, a method or an operator of a handle, which serves every role.
    /// The types of its arguments say what each names.
    pub fn method<M, H, Args, R, F: MethodFn<M, H, Args, R>>(
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
        if spec.kind == MemberKind::Method {
            assert!(
                spec.has_form_of(F::ARITY),
                "a form of `{}` takes the {} arguments its function takes",
                spec.name,
                F::ARITY
            );
        }
        f.register(self.host.engine_mut(), &spec.kind.bound_name(spec.name));
        self.record_runs(spec, F::output());
        self
    }

    /// Records `spec`, which runs, of a form that gives a value of the type `output`.
    fn record_runs(&mut self, spec: MemberSpec, output: TypeId) {
        self.api.record(ApiMember {
            spec,
            writable: false,
            status: Status::Runs(ApiVersion::FIRST),
            output: Some(output),
        });
    }

    /// Binds `f` as the setter of `spec`, a field of a handle `H` scripts may write with a `V`.
    pub fn bind_set<H: ScriptType, V: ScriptArg>(
        &mut self,
        spec: MemberSpec,
        f: impl Fn(&mut H, V) -> Checked<()> + 'static,
    ) -> &mut Self {
        let spec = self.owned(spec);
        debug_assert_eq!(spec.kind, MemberKind::Field, "only a field is written");
        const {
            assert!(
                !V::NUMBER,
                "a setter takes no `Num`, whose integer form it does not bind"
            );
        }
        self.host.engine_mut().register_fn(
            format!("{SETTER}{}", spec.name),
            move |call: NativeCallContext<'_>, handle: &mut H, value: V::Raw| {
                f(handle, V::take(value, &ArgCtx::of_call(&call))?)
            },
        );
        self.api.record(ApiMember {
            spec,
            writable: true,
            status: Status::Runs(ApiVersion::FIRST),
            output: None,
        });
        self
    }

    /// Records `spec` as planned: the design gives it, and no code runs it yet.
    pub fn plan(&mut self, spec: MemberSpec) -> &mut Self {
        let spec = self.owned(spec);
        self.api.record(ApiMember {
            spec,
            writable: false,
            status: Status::Planned,
            output: None,
        });
        self
    }

    /// Records whether the release calls `hook`, as the code that calls it says, of the
    /// capability of the module that calls it and binds.
    pub fn hook(&mut self, hook: Hook, status: Status) -> &mut Self {
        self.api.record_hook(HookStatus {
            hook,
            status,
            capability: self.capability,
            planned_in: None,
        });
        self
    }

    /// Records `hook` as planned with `planned`, a capability the release does not build.
    pub fn plan_hook(&mut self, hook: Hook, planned: PlannedCapability) -> &mut Self {
        self.api.record_hook(HookStatus {
            hook,
            status: Status::Planned,
            capability: None,
            planned_in: Some(planned),
        });
        self
    }

    /// Records whether the release honours `property`, as the code that honours it says.
    pub fn tag_property(&mut self, property: TagProperty, status: Status) -> &mut Self {
        self.api.record_tag_property(property, status);
        self
    }

    /// Records the fields of `T`'s table the release reads, `runs`, and those it plans.
    pub fn data<T: DataSection>(
        &mut self,
        runs: &[&'static str],
        planned: &[&'static str],
    ) -> &mut Self {
        self.api
            .record_data(T::TABLE, runs, planned, self.capability);
        self
    }

    /// Records `key`, a member of an open set of `table` that the module registers, as a field
    /// of `table` the release reads.
    pub fn listed(&mut self, table: DataTable, key: &'static str) -> &mut Self {
        self.api.record_data(table, &[key], &[], self.capability);
        self
    }

    /// Records the fields of `table`, the mode's, a unit type's or an effect's, which many
    /// modules share, that the release reads, `runs`, and those it plans: the module's own.
    pub fn shared_data(
        &mut self,
        table: DataTable,
        runs: &[&'static str],
        planned: &[&'static str],
    ) -> &mut Self {
        assert!(
            matches!(
                table,
                DataTable::Mode | DataTable::UnitType | DataTable::Effect
            ),
            "{table:?} is one module's, which `data` records by its type"
        );
        self.api.record_data(table, runs, planned, self.capability);
        self
    }

    /// Records `fields` of an action's data, each by its name, which the release runs since
    /// `since`.
    pub fn action_fields(
        &mut self,
        since: ApiVersion,
        fields: impl IntoIterator<Item = &'static str>,
    ) -> &mut Self {
        for name in fields {
            self.api.record_field(DataField {
                table: DataTable::Action,
                name,
                status: Status::Runs(since),
                capability: self.capability,
            });
        }
        self
    }

    /// Binds `get` as the indexer of `T`, through which scripts read names the data declares, as
    /// `ctx.p.<name>`, and as the getter of each such name scripts use: the load check reads those
    /// names against the data, and each member that gives a `T` leads to data. `get` has the
    /// running call's `ctx`, found when it reads it.
    pub fn index<T: ScriptType>(
        &mut self,
        get: fn(&ArgCtx<'_>, &mut T, &str) -> Checked<ScriptValue>,
    ) -> &mut Self {
        self.api.record_indexed::<T>();
        self.host.engine_mut().register_fn(
            INDEX_GETTER,
            move |call: NativeCallContext<'_>, target: &mut T, name: ImmutableString| {
                get(&ArgCtx::of_call(&call), target, &name).map(ScriptValue::into_engine)
            },
        );
        self.host.forward_properties(move |engine, name| {
            let property = ImmutableString::from(name);
            engine.register_fn(
                format!("{GETTER}{name}"),
                move |call: NativeCallContext<'_>, target: &mut T| {
                    get(&ArgCtx::of_call(&call), target, &property).map(ScriptValue::into_engine)
                },
            );
        });
        self
    }

    /// Binds `set` as the indexer of `T` that writes, as `ctx.state.<name> = value`, and as the
    /// setter of each such name scripts use. `set` has the running call's context, as `index`'s
    /// getter does.
    pub fn index_set<T: ScriptType>(
        &mut self,
        set: fn(&ArgCtx<'_>, &mut T, &str, ScriptValue) -> Checked<()>,
    ) -> &mut Self {
        self.api.record_indexed::<T>();
        self.host.engine_mut().register_fn(
            INDEX_SETTER,
            move |call: NativeCallContext<'_>,
                  target: &mut T,
                  name: ImmutableString,
                  value: Dynamic| {
                let value = ScriptValue::of_engine(value);
                set(&ArgCtx::of_call(&call), target, &name, value)
            },
        );
        self.host.forward_properties(move |engine, name| {
            let property = ImmutableString::from(name);
            engine.register_fn(
                format!("{SETTER}{name}"),
                move |call: NativeCallContext<'_>, target: &mut T, value: Dynamic| {
                    let value = ScriptValue::of_engine(value);
                    set(&ArgCtx::of_call(&call), target, &property, value)
                },
            );
        });
        self
    }
}
