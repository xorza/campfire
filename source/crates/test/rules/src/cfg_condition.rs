use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ExprLit, Lit, Macro, Meta, Token};

/// A condition of a source's code: the predicate of a `cfg`, a `cfg_attr` or a `cfg!`, at the
/// line it starts.
#[derive(Debug)]
pub(crate) struct CfgCondition {
    pub(crate) line: usize,
    /// Whether it tests the `internals` feature, alone or within another predicate.
    pub(crate) tests_internals: bool,
    /// Whether it is `any(test, feature = "internals")`, in either order.
    pub(crate) test_or_internals: bool,
}

impl CfgCondition {
    /// The conditions of `code`, in its order.
    pub(crate) fn of(code: &str) -> Vec<Self> {
        let file = syn::parse_file(code).expect("a workspace source parses");
        let mut conditions = CfgConditions(Vec::new());
        conditions.visit_file(&file);
        conditions.0
    }

    fn read(predicate: &Meta) -> Self {
        let test_or_internals = parts(predicate, "any").is_some_and(|parts| {
            parts.len() == 2
                && parts.iter().any(|part| part.path().is_ident("test"))
                && parts.iter().any(is_internals)
        });
        Self {
            line: predicate.span().start().line,
            tests_internals: holds_internals(predicate),
            test_or_internals,
        }
    }
}

fn holds_internals(meta: &Meta) -> bool {
    is_internals(meta)
        || ["any", "all", "not"]
            .iter()
            .any(|name| parts(meta, name).is_some_and(|parts| parts.iter().any(holds_internals)))
}

fn is_internals(meta: &Meta) -> bool {
    matches!(
        meta,
        Meta::NameValue(pair) if pair.path.is_ident("feature")
            && matches!(&pair.value, Expr::Lit(ExprLit { lit: Lit::Str(name), .. }) if name.value() == "internals")
    )
}

/// The predicates `meta` joins, when it is the combinator `name`.
fn parts(meta: &Meta, name: &str) -> Option<Vec<Meta>> {
    let Meta::List(list) = meta else {
        return None;
    };
    if !list.path.is_ident(name) {
        return None;
    }
    let parts = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()?;
    Some(parts.into_iter().collect())
}

/// The conditions a walk of one file found.
#[derive(Debug)]
struct CfgConditions(Vec<CfgCondition>);

impl CfgConditions {
    /// Takes the predicate that starts `metas`, the arguments of a `cfg`, `cfg_attr` or `cfg!`.
    fn take(&mut self, metas: syn::Result<Punctuated<Meta, Token![,]>>) {
        if let Some(predicate) = metas.ok().and_then(|metas| metas.into_iter().next()) {
            self.0.push(CfgCondition::read(&predicate));
        }
    }
}

impl<'ast> Visit<'ast> for CfgConditions {
    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        let path = attribute.path();
        if path.is_ident("cfg") || path.is_ident("cfg_attr") {
            self.take(attribute.parse_args_with(Punctuated::parse_terminated));
        }
    }

    fn visit_macro(&mut self, item: &'ast Macro) {
        if item.path.is_ident("cfg") {
            self.take(item.parse_body_with(Punctuated::parse_terminated));
        }
        visit::visit_macro(self, item);
    }
}

#[test]
fn a_condition_is_read_from_each_cfg_and_never_from_a_comment() {
    let code = "/// #[cfg(feature = \"internals\")]\n\
        #[cfg(any(test, feature = \"internals\"))]\nmod a;\n\
        #[cfg_attr(\n    any(feature = \"internals\", test),\n    derive(Debug)\n)]\nstruct B;\n\
        #[cfg(all(unix, not(feature = \"internals\")))]\nfn c() { cfg!(feature = \"other\"); }\n\
        #[cfg(any(test, feature = \"internals\", unix))]\nmod d;\n";
    let conditions: Vec<(usize, bool, bool)> = CfgCondition::of(code)
        .iter()
        .map(|condition| {
            let line = condition.line;
            (line, condition.tests_internals, condition.test_or_internals)
        })
        .collect();
    assert_eq!(
        conditions,
        [
            (2, true, true),
            (5, true, true),
            (9, true, false),
            (10, false, false),
            (11, true, false),
        ]
    );
}
