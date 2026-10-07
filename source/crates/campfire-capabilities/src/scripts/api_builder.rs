use campfire_script::ScriptHost;
use campfire_script::rhai::{
    Dynamic, ImmutableString, Module, NativeCallContext, RhaiNativeFunc, Variant,
};

use crate::scripts::api_version::ApiVersion;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::enum_record::EnumRecord;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::scripts::script_api::{HookStatus, MemberKind, ScriptApi};
use crate::units::tag_property::TagProperty;
use crate::values::script_enum::ScriptEnum;

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
    host: &'a mut ScriptHost,
    api: &'a mut ScriptApi,
}

impl<'a> ApiBuilder<'a> {
    pub(crate) const fn new(host: &'a mut ScriptHost, api: &'a mut ScriptApi) -> ApiBuilder<'a> {
        ApiBuilder { host, api }
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
        self.host.engine_mut().register_fn(name, f);
        self.api
            .record(spec, false, Status::Runs(ApiVersion::FIRST));
        self
    }

    /// Binds `f` as the setter of `spec`, a field scripts may write.
    pub(crate) fn bind_set<A: 'static, const X: bool, const F: bool>(
        &mut self,
        spec: MemberSpec,
        f: impl RhaiNativeFunc<A, 2, X, (), F> + 'static,
    ) -> &mut Self {
        debug_assert_eq!(spec.kind, MemberKind::Field, "only a field is written");
        self.host
            .engine_mut()
            .register_fn(format!("{SETTER}{}", spec.name), f);
        self.api.record(spec, true, Status::Runs(ApiVersion::FIRST));
        self
    }

    /// Records `spec` as planned: design 08 gives it, and no code runs it yet.
    pub(crate) fn plan(&mut self, spec: MemberSpec) -> &mut Self {
        self.api.record(spec, false, Status::Planned);
        self
    }

    /// Records whether the release calls `hook`, as the code that calls it says.
    pub(crate) fn hook(&mut self, hook: Hook, status: Status) -> &mut Self {
        self.api.record_hook(HookStatus { hook, status });
        self
    }

    /// Records whether the release honours `property`, as the code that honours it says.
    pub(crate) fn tag_property(&mut self, property: TagProperty, status: Status) -> &mut Self {
        self.api.record_tag_property(property, status);
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

    /// Records `fields` of an action's data, each by its name with its status.
    pub(crate) fn action_fields(
        &mut self,
        fields: impl IntoIterator<Item = (&'static str, Status)>,
    ) -> &mut Self {
        for (name, status) in fields {
            self.api.record_field(DataTable::Action, name, status);
        }
        self
    }

    /// Binds `get` as the indexer of `T`, through which scripts read names the data declares, as
    /// `ctx.p.<name>`, and as the getter of each such name scripts use: the load check reads those
    /// names against the data.
    pub(crate) fn index<T: Variant + Clone>(
        &mut self,
        get: fn(&mut T, &str) -> Checked<Dynamic>,
    ) -> &mut Self {
        self.host.engine_mut().register_fn(
            INDEX_GETTER,
            move |target: &mut T, name: ImmutableString| get(target, &name),
        );
        self.host.forward_properties(move |engine, name| {
            let property = ImmutableString::from(name);
            engine.register_fn(format!("{GETTER}{name}"), move |target: &mut T| {
                get(target, &property)
            });
        });
        self
    }

    /// Binds `get` as `index` does, for a `T` that reads the running call's context.
    pub(crate) fn index_in_call<T: Variant + Clone>(
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
    /// setter of each such name scripts use.
    pub(crate) fn index_set<T: Variant + Clone>(
        &mut self,
        set: fn(&mut T, &str, Dynamic) -> Checked<()>,
    ) -> &mut Self {
        self.host.engine_mut().register_fn(
            INDEX_SETTER,
            move |target: &mut T, name: ImmutableString, value: Dynamic| set(target, &name, value),
        );
        self.host.forward_properties(move |engine, name| {
            let property = ImmutableString::from(name);
            engine.register_fn(
                format!("{SETTER}{name}"),
                move |target: &mut T, value: Dynamic| set(target, &property, value),
            );
        });
        self
    }

    /// Binds `set` as `index_set` does, for a `T` that reads the running call's context.
    pub(crate) fn index_set_in_call<T: Variant + Clone>(
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
