//! The script runtime under every capability: the Rhai host, the script API and its registry,
//! the one `ctx` every call goes through, the batch every call runs in, its budgets and
//! failures, script state, and the script view the modules fill.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod script_host;
mod scripts;

pub use crate::script_host::budget::Budget;
pub use crate::script_host::error::{NumError, Raised, ScriptError};
pub use crate::script_host::{ScriptHost, ScriptId};
pub use crate::scripts::api_owner::ApiOwner;
pub use crate::scripts::api_version::ApiVersion;
pub use crate::scripts::applies::Applies;
pub use crate::scripts::arg_ctx::ArgCtx;
pub use crate::scripts::call_part::{CallParams, CallPart, Commit};
pub use crate::scripts::call_start::{CallStart, Purity};
pub use crate::scripts::capability_refusal::CapabilityRefusal;
pub use crate::scripts::core_api::CoreApi;
pub use crate::scripts::ctx::{Ctx, Params};
pub use crate::scripts::ctx_fn::{CtxFn, Fallible, GetFn, MethodFn, Plain, WithCtx, WithoutCtx};
pub use crate::scripts::data_section::DataSection;
pub use crate::scripts::data_table::DataTable;
pub use crate::scripts::error::{
    ApiError, CallError, Checked, CtxMisuse, ParamProblem, Refusal, UnknownName,
};
pub use crate::scripts::frame::Frame;
pub use crate::scripts::frame::effects::{Effect, Effects};
pub use crate::scripts::hook::Hook;
pub use crate::scripts::hook_set::HookSet;
pub use crate::scripts::member_kind::MemberKind;
pub use crate::scripts::member_spec::{ArgKinds, EnumArgs, Forms, MemberSpec, NameArgs};
pub use crate::scripts::name_kind::NameKind;
pub use crate::scripts::pending_calls::PendingCalls;
pub use crate::scripts::pool::Pool;
pub use crate::scripts::role_set::RoleSet;
pub use crate::scripts::script_api::ScriptApi;
pub use crate::scripts::script_api::api_builder::ApiBuilder;
pub use crate::scripts::script_api::api_member::ApiMember;
pub use crate::scripts::script_api::data_field::DataField;
pub use crate::scripts::script_api::enum_record::EnumRecord;
pub use crate::scripts::script_api::hook_status::HookStatus;
pub use crate::scripts::script_api::tag_property_status::TagPropertyStatus;
pub use crate::scripts::script_arg::{ScriptArg, TextArg};
pub use crate::scripts::script_args::ScriptArgs;
pub use crate::scripts::script_batch::{HookCall, ScriptBatch};
pub use crate::scripts::script_budgets::ScriptBudgets;
pub use crate::scripts::script_call::ScriptCall;
pub use crate::scripts::script_cut::ScriptCut;
pub use crate::scripts::script_facts::{
    CtxUse, EnumPath, EnumString, MemberUse, ScriptFacts, ScriptFunction, ScriptName,
};
pub use crate::scripts::script_failures::{ScriptFailure, ScriptFailures};
pub use crate::scripts::script_fn::ScriptFn;
pub use crate::scripts::script_lasting::ScriptLasting;
pub use crate::scripts::script_limits::ScriptLimits;
pub use crate::scripts::script_names::ScriptNames;
pub use crate::scripts::script_pools::ScriptPools;
pub use crate::scripts::script_position::ScriptPosition;
pub use crate::scripts::script_radius::ScriptRadius;
pub use crate::scripts::script_role::{ParamScope, ScriptRole};
pub use crate::scripts::script_text::ScriptText;
pub use crate::scripts::script_ticks::ScriptTicks;
pub use crate::scripts::script_value::script_list::ScriptList;
pub use crate::scripts::script_value::script_map::ScriptMap;
pub use crate::scripts::script_value::{ScriptType, ScriptValue};
pub use crate::scripts::script_window::ScriptWindow;
pub use crate::scripts::scripts_world::ScriptsWorld;
pub use crate::scripts::state_decl::state_schema::{StateField, StatePlace, StateSchema};
pub use crate::scripts::state_decl::synced_state_decl::{SyncTo, SyncedStateDecl};
pub use crate::scripts::state_decl::{StateDecl, StateDefault};
pub use crate::scripts::state_type::StateType;
pub use crate::scripts::state_value::StateValue;
pub use crate::scripts::status::Status;
pub use crate::scripts::view::View;
pub use crate::scripts::view::filter::Filter;
pub use crate::scripts::view::filters::Filters;
pub use crate::scripts::view::kept_rows::{AnyKeptRows, ColumnRows, KeptRows, RunMove};
pub use crate::scripts::view::living_unit::LivingUnit;
pub use crate::scripts::view::row_fill::RowFill;
pub use crate::scripts::view::row_reads::RowReads;
pub use crate::scripts::view::tag_names::TagNames;
pub use crate::scripts::view::teams::Teams;
pub use crate::scripts::view::unit::Unit;
pub use crate::scripts::view::unit_row::UnitRow;
pub use crate::scripts::view::view_column::{ViewColumn, ViewColumns};
pub use crate::scripts::view::view_names::ViewNames;
pub use crate::scripts::view::view_rows::{ReadOf, ViewRows};
pub use crate::scripts::view_api::ViewApi;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::script_host::error::internals::Undefined;
    pub use crate::script_host::internals::BoundFunction;
    pub use crate::scripts::error::internals::FailureKind;
    pub use crate::scripts::script_failures::internals::FailedCall;
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::scripts::scripts_world::internals::ScriptsWorldInternals;
}

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::script_host;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        script_host::bench::script(c);
    }
}
