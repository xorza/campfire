use std::fmt::Write;

use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;

/// A name of the script API: its spec, as every binding of it gives it, whether a script may
/// write it, as `m.stacks`, and whether it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiMember {
    pub spec: MemberSpec,
    pub writable: bool,
    pub status: Status,
}

impl ApiMember {
    /// Its arguments that name something or take an engine enum, as the reference lists them
    /// after its forms: `, `id` a modifier`, each by its name in the first form that has it.
    pub(super) fn name_args_text(&self) -> String {
        let spec = &self.spec;
        let arg = |at: usize| {
            let form = spec.forms.iter().find(|form| at < form.len());
            form.expect("a form has each argument that names something")[at]
        };
        let mut named = String::new();
        for (at, kind) in spec.names.iter().enumerate() {
            if let Some(kind) = kind {
                write!(named, ", `{}` a {kind}", arg(at)).expect("text writes into a string");
            }
        }
        for (at, engine_enum) in spec.enums.iter().enumerate() {
            if let Some(engine_enum) = engine_enum {
                write!(named, ", `{}` a `{engine_enum}`", arg(at))
                    .expect("text writes into a string");
            }
        }
        named
    }
}
