use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    Arm, Attribute, Expr, Field, FieldValue, ForeignItem, ImplItem, Item, Stmt, TraitItem, Variant,
};

use crate::source_file::Gate;

/// The items of a parsed source that a gate holds, each whole with its attributes and doc
/// comments, at any depth: an item, a statement, a field, a variant, an arm, a field's value or an
/// expression. An item inside a gated one is found too, as a gate inside `internals` code holds
/// what the library leaves out.
#[derive(Debug, Default)]
pub(crate) struct GatedItems {
    found: Vec<(Gate, Span)>,
}

impl GatedItems {
    /// Each gated item the visit found, with its gate.
    pub(crate) fn found(self) -> Vec<(Gate, Span)> {
        self.found
    }

    /// Records `node` when one of `attributes` gates it.
    fn record(&mut self, attributes: &[Attribute], node: &impl Spanned) {
        if let Some(gate) = attributes.iter().find_map(Gate::of) {
            self.found.push((gate, node.span()));
        }
    }
}

impl<'ast> Visit<'ast> for GatedItems {
    fn visit_item(&mut self, item: &'ast Item) {
        self.record(item_attributes(item), item);
        visit::visit_item(self, item);
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        let attributes: &[Attribute] = match item {
            ImplItem::Const(item) => &item.attrs,
            ImplItem::Fn(item) => &item.attrs,
            ImplItem::Type(item) => &item.attrs,
            ImplItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.record(attributes, item);
        visit::visit_impl_item(self, item);
    }

    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        let attributes: &[Attribute] = match item {
            TraitItem::Const(item) => &item.attrs,
            TraitItem::Fn(item) => &item.attrs,
            TraitItem::Type(item) => &item.attrs,
            TraitItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.record(attributes, item);
        visit::visit_trait_item(self, item);
    }

    fn visit_foreign_item(&mut self, item: &'ast ForeignItem) {
        let attributes: &[Attribute] = match item {
            ForeignItem::Fn(item) => &item.attrs,
            ForeignItem::Static(item) => &item.attrs,
            ForeignItem::Type(item) => &item.attrs,
            ForeignItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.record(attributes, item);
        visit::visit_foreign_item(self, item);
    }

    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        let attributes: &[Attribute] = match statement {
            Stmt::Local(local) => &local.attrs,
            Stmt::Macro(made) => &made.attrs,
            Stmt::Expr(expression, _) => expression_attributes(expression),
            Stmt::Item(_) => &[],
        };
        self.record(attributes, statement);
        visit::visit_stmt(self, statement);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        self.record(expression_attributes(expression), expression);
        visit::visit_expr(self, expression);
    }

    fn visit_field(&mut self, field: &'ast Field) {
        self.record(&field.attrs, field);
        visit::visit_field(self, field);
    }

    fn visit_variant(&mut self, variant: &'ast Variant) {
        self.record(&variant.attrs, variant);
        visit::visit_variant(self, variant);
    }

    fn visit_arm(&mut self, arm: &'ast Arm) {
        self.record(&arm.attrs, arm);
        visit::visit_arm(self, arm);
    }

    fn visit_field_value(&mut self, value: &'ast FieldValue) {
        self.record(&value.attrs, value);
        visit::visit_field_value(self, value);
    }
}

/// The outer attributes of `item`.
fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

/// The outer attributes of `expression`.
fn expression_attributes(expression: &Expr) -> &[Attribute] {
    match expression {
        Expr::Array(expression) => &expression.attrs,
        Expr::Assign(expression) => &expression.attrs,
        Expr::Async(expression) => &expression.attrs,
        Expr::Await(expression) => &expression.attrs,
        Expr::Binary(expression) => &expression.attrs,
        Expr::Block(expression) => &expression.attrs,
        Expr::Break(expression) => &expression.attrs,
        Expr::Call(expression) => &expression.attrs,
        Expr::Cast(expression) => &expression.attrs,
        Expr::Closure(expression) => &expression.attrs,
        Expr::Const(expression) => &expression.attrs,
        Expr::Continue(expression) => &expression.attrs,
        Expr::Field(expression) => &expression.attrs,
        Expr::ForLoop(expression) => &expression.attrs,
        Expr::Group(expression) => &expression.attrs,
        Expr::If(expression) => &expression.attrs,
        Expr::Index(expression) => &expression.attrs,
        Expr::Infer(expression) => &expression.attrs,
        Expr::Let(expression) => &expression.attrs,
        Expr::Lit(expression) => &expression.attrs,
        Expr::Loop(expression) => &expression.attrs,
        Expr::Macro(expression) => &expression.attrs,
        Expr::Match(expression) => &expression.attrs,
        Expr::MethodCall(expression) => &expression.attrs,
        Expr::Paren(expression) => &expression.attrs,
        Expr::Path(expression) => &expression.attrs,
        Expr::Range(expression) => &expression.attrs,
        Expr::RawAddr(expression) => &expression.attrs,
        Expr::Reference(expression) => &expression.attrs,
        Expr::Repeat(expression) => &expression.attrs,
        Expr::Return(expression) => &expression.attrs,
        Expr::Struct(expression) => &expression.attrs,
        Expr::Try(expression) => &expression.attrs,
        Expr::TryBlock(expression) => &expression.attrs,
        Expr::Tuple(expression) => &expression.attrs,
        Expr::Unary(expression) => &expression.attrs,
        Expr::Unsafe(expression) => &expression.attrs,
        Expr::While(expression) => &expression.attrs,
        Expr::Yield(expression) => &expression.attrs,
        _ => &[],
    }
}
