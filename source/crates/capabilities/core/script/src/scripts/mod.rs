//! The script runtime under every capability: the script API and its registry, the one `ctx`
//! every call goes through, the batch every call runs in, its budgets and failures, script state,
//! and the script view the modules fill.

pub(crate) mod api_builder;
pub(crate) mod api_version;
pub(crate) mod applies;
pub(crate) mod arg_ctx;
pub(crate) mod call_part;
pub(crate) mod call_start;
pub(crate) mod core_api;
pub(crate) mod ctx;
pub(crate) mod ctx_fn;
pub(crate) mod data_section;
pub(crate) mod error;
pub(crate) mod frame;
pub(crate) mod hook;
pub(crate) mod hook_set;
mod id_args;
pub(crate) mod name_kind;
pub(crate) mod pending_calls;
pub(crate) mod pool;
pub(crate) mod role_set;
pub(crate) mod script_api;
pub(crate) mod script_arg;
pub(crate) mod script_args;
pub(crate) mod script_batch;
pub(crate) mod script_budgets;
pub(crate) mod script_call;
pub(crate) mod script_facts;
pub(crate) mod script_failures;
pub(crate) mod script_fn;
pub(crate) mod script_limits;
pub(crate) mod script_list;
pub(crate) mod script_map;
pub(crate) mod script_names;
pub(crate) mod script_role;
pub(crate) mod script_text;
pub(crate) mod script_value;
pub(crate) mod scripts_world;
pub(crate) mod state_decl;
pub(crate) mod state_value;
pub(crate) mod type_slots;
pub(crate) mod view;
