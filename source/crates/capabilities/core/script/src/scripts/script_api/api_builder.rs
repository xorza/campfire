use campfire_codec::StructFields;
use campfire_sim::Capability;
use campfire_values::{ScriptEnum, TagProperty};
use rhai::{Dynamic, ImmutableString, Module, NativeCallContext};

use crate::script_host::ScriptHost;
use crate::scripts::api_holder::ScriptHandle;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::arg_ctx::ArgCtx;
use crate::scripts::ctx_fn::{CtxFn, GetFn, MethodFn};
use crate::scripts::data_section::DataSection;
use crate::scripts::data_table::DataTable;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::{Hook, HookDecl};
use crate::scripts::member_decl::MemberDecl;
use crate::scripts::member_kind::{GETTER, MemberKind};
use crate::scripts::planned_capability::PlannedCapability;
use crate::scripts::script_api::ScriptApi;
use crate::scripts::script_api::api_member::{ApiMember, FormOutput};
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
    fn owned(&self, mut decl: MemberDecl) -> MemberDecl {
        assert!(
            self.capability.is_none() || decl.capability != self.capability,
            "`{}` names the capability of the module that binds it",
            decl.name
        );
        decl.capability = decl.capability.or(self.capability);
        decl
    }

    /// Registers `T` under its name in scripts.
    pub fn ty<T: ScriptHandle>(&mut self) -> &mut Self {
        self.host.engine_mut().register_type_with_name::<T>(T::NAME);
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
    pub fn bind<M, H, O, K, F: GetFn<M, H, O, K>>(&mut self, decl: MemberDecl, f: F) -> &mut Self {
        let decl = self.owned(decl);
        assert!(
            matches!(decl.kind, MemberKind::Value | MemberKind::Field),
            "`{}` takes arguments, which `call` or `method` binds",
            decl.name
        );
        assert_eq!(
            decl.roles,
            decl.kind.every_role(),
            "`{}` serves some roles only, which `call` checks",
            decl.name
        );
        f.register(self.host.engine_mut(), &decl.kind.bound_name(decl.name));
        let output = FormOutput {
            ty: F::output(),
            names: F::NAMES,
        };
        self.record_runs(&decl, output, &[]);
        self
    }

    /// Binds `f`, a form of `spec`, a call or a value of `ctx` that takes `ctx` first, for the
    /// roles of `spec`: a call in another role fails before `f` runs. The types of its arguments
    /// say what each names.
    pub fn call<Args, R, F: CtxFn<Args, R>>(&mut self, decl: MemberDecl, f: F) -> &mut Self {
        let decl = self.owned(decl).with_args(F::args());
        assert!(
            matches!(decl.kind, MemberKind::Call | MemberKind::Value),
            "`{}` is no call or value of `ctx`",
            decl.name
        );
        assert!(
            decl.has_form_of(F::ARITY),
            "a form of `{}` takes the {} arguments its function takes",
            decl.name,
            F::ARITY
        );
        f.register(
            self.host.engine_mut(),
            &decl.kind.bound_name(decl.name),
            decl.roles,
        );
        let output = FormOutput {
            ty: F::output(),
            names: F::NAMES,
        };
        self.record_runs(&decl, output, F::TYPES);
        self
    }

    /// Binds `f`, a form of `spec`, a method or an operator of a handle, which serves every role.
    /// The types of its arguments say what each names.
    pub fn method<M, H, Args, R, F: MethodFn<M, H, Args, R>>(
        &mut self,
        decl: MemberDecl,
        f: F,
    ) -> &mut Self {
        let decl = self.owned(decl).with_args(F::args());
        assert!(
            matches!(decl.kind, MemberKind::Method | MemberKind::Operator),
            "`{}` is no method or operator",
            decl.name
        );
        assert_eq!(
            decl.roles,
            decl.kind.every_role(),
            "`{}`, a method, serves every role",
            decl.name
        );
        if decl.kind == MemberKind::Method {
            assert!(
                decl.has_form_of(F::ARITY),
                "a form of `{}` takes the {} arguments its function takes",
                decl.name,
                F::ARITY
            );
        }
        f.register(self.host.engine_mut(), &decl.kind.bound_name(decl.name));
        let output = FormOutput {
            ty: F::output(),
            names: F::NAMES,
        };
        self.record_runs(&decl, output, F::TYPES);
        self
    }

    /// Records `spec`, which runs, of a form that gives `output`.
    fn record_runs(&mut self, decl: &MemberDecl, output: FormOutput, types: &[&'static str]) {
        self.api.record(ApiMember {
            decl: *decl,
            writable: false,
            status: Status::Runs(decl.since),
            output: Some(output),
            types: types.iter().map(|&ty| vec![ty]).collect(),
        });
    }

    /// Binds `f` as the setter of `spec`, a field of a handle `H` scripts may write with a `V`.
    pub fn bind_set<H: ScriptType, V: ScriptArg>(
        &mut self,
        decl: MemberDecl,
        f: impl Fn(&mut H, V) -> Checked<()> + 'static,
    ) -> &mut Self {
        let decl = self.owned(decl);
        debug_assert_eq!(decl.kind, MemberKind::Field, "only a field is written");
        const {
            assert!(
                !V::NUMBER,
                "a setter takes no `Num`, whose integer form it does not bind"
            );
        }
        self.host.engine_mut().register_fn(
            format!("{SETTER}{}", decl.name),
            move |call: NativeCallContext<'_>, handle: &mut H, value: V::Raw| {
                f(handle, V::take(value, &ArgCtx::of_call(&call))?)
            },
        );
        self.api.record(ApiMember {
            decl,
            writable: true,
            status: Status::Runs(decl.since),
            output: None,
            types: Vec::new(),
        });
        self
    }

    /// Records `spec` as planned: the design gives it, and no code runs it yet.
    pub fn plan(&mut self, decl: MemberDecl) -> &mut Self {
        let decl = self.owned(decl);
        self.api.record(ApiMember {
            decl,
            writable: false,
            status: Status::Planned,
            output: None,
            types: Vec::new(),
        });
        self
    }

    /// Records whether the release calls the hook `H`, as the code that calls it says, of the
    /// capability of the module that calls it and binds.
    pub fn hook<H: HookDecl>(&mut self, status: Status) -> &mut Self {
        self.api.record_hook(HookStatus {
            hook: Hook::of::<H>(),
            status,
            capability: self.capability,
            planned_in: None,
        });
        self
    }

    /// Records the hook `H` as planned with `planned`, a capability the release does not build.
    pub fn plan_hook<H: HookDecl>(&mut self, planned: PlannedCapability) -> &mut Self {
        self.api.record_hook(HookStatus {
            hook: Hook::of::<H>(),
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

    /// Records the fields of `T`'s table, as serde reads them, each of the status `statuses`
    /// gives it, or read since the first version.
    pub fn data<T: DataSection>(&mut self, statuses: &[(&'static str, Status)]) -> &mut Self {
        let fields = StructFields::of::<T>();
        assert!(
            statuses.iter().all(|(name, _)| fields.contains(name)),
            "the fields of {} given a status are fields of its type",
            T::TABLE.title()
        );
        for &name in fields {
            let given = statuses.iter().find(|(held, _)| *held == name);
            let status = given.map_or(Status::Runs(ApiVersion::FIRST), |&(_, status)| status);
            self.record_field(T::TABLE, name, status);
        }
        self
    }

    /// Records `fields`, the module's own fields of `T`'s table, whose type several modules read,
    /// as read since the first version: each a field serde reads of `T`.
    pub fn own_fields<T: DataSection>(&mut self, fields: &[&'static str]) -> &mut Self {
        let read = StructFields::of::<T>();
        assert!(
            fields.iter().all(|name| read.contains(name)),
            "the fields of {} a module owns are fields of its type",
            T::TABLE.title()
        );
        for &name in fields {
            self.record_field(T::TABLE, name, Status::Runs(ApiVersion::FIRST));
        }
        self
    }

    /// Records `key`, a key of `table` that the module declares, a section, a member of an open
    /// set, or a key the package load reads, as the mode file's `script`, as a field of `table`
    /// of `status`.
    pub fn key(&mut self, table: DataTable, key: &'static str, status: Status) -> &mut Self {
        self.record_field(table, key, status);
        self
    }

    /// Records the field `name` of `table`, of `status` and of the module's capability.
    fn record_field(&mut self, table: DataTable, name: &'static str, status: Status) {
        self.api.record_field(DataField {
            table,
            name,
            status,
            capability: self.capability,
        });
    }

    /// Records `fields` of `table`, each by its name, which the release reads since `since`: a
    /// table whose fields several modules give, as an action's, each its own.
    pub fn fields(
        &mut self,
        table: DataTable,
        since: ApiVersion,
        fields: impl IntoIterator<Item = &'static str>,
    ) -> &mut Self {
        for name in fields {
            self.record_field(table, name, Status::Runs(since));
        }
        self
    }

    /// Binds `get` as the indexer of `T`, through which scripts read names the data declares, as
    /// `ctx.params.<name>`, and as the getter of each such name scripts use: the load check reads
    /// those names against the data, and each member that gives a `T` leads to data. `get` has the
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
