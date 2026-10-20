use std::any::TypeId;
use std::fmt::Write;

use crate::scripts::given_names::GivenNames;
use crate::scripts::member_decl::MemberDecl;
use crate::scripts::status::Status;

/// A name of the script API: its spec, as every binding of it gives it, whether a script may
/// write it, as `m.stacks`, whether it runs, what its forms give, none for a planned name and a
/// setter alone, and the names of the types its forms take at each place of their arguments, in
/// the order bound, as their closures' signatures say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiMember {
    pub decl: MemberDecl,
    pub writable: bool,
    pub status: Status,
    pub output: Option<FormOutput>,
    pub types: Vec<Vec<&'static str>>,
}

/// What a form gives, as its closure's signature says: the type of the value, and the kind of
/// name it is, none for a value that names nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormOutput {
    pub ty: TypeId,
    pub names: Option<GivenNames>,
}

impl ApiMember {
    /// The argument `name` at `at` of a form, with the types its forms take there, as the
    /// reference shows it: `to: Vec3 or Unit`.
    pub(super) fn typed_arg(&self, at: usize, name: &str) -> String {
        match self.types.get(at) {
            Some(types) if !types.is_empty() => format!("{name}: {}", types.join(" or ")),
            _ => name.to_owned(),
        }
    }

    /// Takes in the types of `form`, a binding of the same name, at each place.
    pub(super) fn take_types(&mut self, form: &[Vec<&'static str>]) {
        if self.types.len() < form.len() {
            self.types.resize_with(form.len(), Vec::new);
        }
        for (held, types) in self.types.iter_mut().zip(form) {
            for &ty in types {
                if !held.contains(&ty) {
                    held.push(ty);
                }
            }
        }
    }

    /// Its arguments that name something or take an engine enum, as the reference lists them
    /// after its forms: `, `id` a modifier`, each by its name in the first form that has it.
    pub(super) fn name_args_text(&self) -> String {
        let spec = &self.decl;
        let arg = |at: usize| {
            let form = spec.forms.iter().find(|form| at < form.len());
            form.expect("a form has each argument that names something")[at]
        };
        let mut named = String::new();
        for (at, kind) in spec.names.iter().enumerate() {
            if let Some(kind) = kind {
                write!(named, ", `{}` {} {kind}", arg(at), kind.article())
                    .expect("text writes into a string");
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
