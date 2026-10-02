//! The script runtime under every capability: the script API and its registry, the one `ctx`
//! every call goes through, the batch every call runs in, its budgets and failures, and script
//! state.

pub(crate) mod api_builder;
pub(crate) mod api_version;
pub(crate) mod call_part;
pub(crate) mod call_start;
pub(crate) mod core_api;
pub(crate) mod ctx;
pub(crate) mod effects;
pub(crate) mod error;
pub(crate) mod frame;
pub(crate) mod hook;
pub(crate) mod hook_set;
pub(crate) mod name_kind;
pub(crate) mod pool;
pub(crate) mod role_set;
pub(crate) mod script_api;
pub(crate) mod script_batch;
pub(crate) mod script_book;
pub(crate) mod script_budgets;
pub(crate) mod script_consts;
pub(crate) mod script_failures;
pub(crate) mod script_limits;
pub(crate) mod script_role;
pub(crate) mod state_decl;
pub(crate) mod state_value;
