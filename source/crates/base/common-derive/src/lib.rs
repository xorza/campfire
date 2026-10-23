//! The workspace's derives: `EnumAll`, an enum's array of its variants, `EnumIdents`, their
//! identifiers, and `NamedEnum`, the names data gives them (02-engine-core.md, A name stays
//! beside its member).

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::quote;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{
    Data, DeriveInput, Error, Expr, ExprLit, Fields, Ident, Lit, LitStr, Meta, Variant,
    parse_macro_input,
};

/// `ALL`, an array of every variant in the order declared, as visible as the enum. Its
/// variants hold no data, and may name their discriminants.
#[proc_macro_derive(EnumAll)]
pub fn derive_enum_all(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    enum_all(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// `IDENTS`, each variant's identifier as written, in the order declared, as visible as the
/// enum.
#[proc_macro_derive(EnumIdents)]
pub fn derive_enum_idents(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    enum_idents(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// `campfire_common::NamedEnum`, from the name each variant has, which data writes and reads it
/// by: its identifier in snake case, as serde's `rename_all = "snake_case"` writes it, or the
/// name `#[name = "<name>"]` gives it in its place; and `ALL`, as `EnumAll` gives it, the one
/// list of its members, whose place is each one's discriminant. An empty name, two variants of
/// one name, or a discriminant of its own, fail to compile.
#[proc_macro_derive(NamedEnum, attributes(name))]
pub fn derive_named_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    named_enum(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn enum_all(input: &DeriveInput) -> syn::Result<Tokens> {
    let variants = variants(input)?;
    let DeriveInput {
        vis: visibility,
        ident: name,
        ..
    } = input;
    let count = variants.len();
    let members = variants.iter().map(|variant| &variant.ident);
    Ok(quote! {
        impl #name {
            /// Every variant, in the order declared.
            #visibility const ALL: [Self; #count] = [#(Self::#members),*];
        }
    })
}

fn enum_idents(input: &DeriveInput) -> syn::Result<Tokens> {
    let variants = variants(input)?;
    let DeriveInput {
        vis: visibility,
        ident: name,
        ..
    } = input;
    let count = variants.len();
    let written = variants
        .iter()
        .map(|variant| variant.ident.unraw().to_string());
    Ok(quote! {
        impl #name {
            /// Each variant's identifier as written, in the order declared.
            #visibility const IDENTS: [&'static str; #count] = [#(#written),*];
        }
    })
}

fn named_enum(input: &DeriveInput) -> syn::Result<Tokens> {
    let variants = variants(input)?;
    if let Some((_, discriminant)) = variants
        .iter()
        .find_map(|variant| variant.discriminant.as_ref())
    {
        return Err(Error::new(
            discriminant.span(),
            "a member of no discriminant of its own, so its place is its discriminant",
        ));
    }
    let mut names: Vec<LitStr> = Vec::with_capacity(variants.len());
    for variant in &variants {
        let name = name_of(variant)?
            .unwrap_or_else(|| LitStr::new(&snake(&variant.ident), variant.ident.span()));
        if let Some(first) = names.iter().find(|named| named.value() == name.value()) {
            let mut error = Error::new(name.span(), "a second member of this name");
            error.combine(Error::new(first.span(), "its first"));
            return Err(error);
        }
        names.push(name);
    }
    let name = &input.ident;
    let type_name = name.to_string();
    let all = enum_all(input)?;
    let visibility = &input.vis;
    let members = variants.iter().map(|variant| &variant.ident);
    Ok(quote! {
        #all

        impl #name {
            /// The member's name, as `NamedEnum::NAMES` gives it, in a const context too.
            #visibility const fn name(self) -> &'static str {
                match self {
                    #(Self::#members => #names,)*
                }
            }
        }

        impl ::campfire_common::NamedEnum for #name {
            const TYPE: &'static str = #type_name;
            const ALL: &'static [Self] = &Self::ALL;
            const NAMES: &'static [&'static str] = &[#(#names),*];

            fn index(self) -> usize {
                self as usize
            }
        }
    })
}

/// The variants of `input`, an enum of no generics whose variants hold no data.
fn variants(input: &DeriveInput) -> syn::Result<Vec<&Variant>> {
    let Data::Enum(data) = &input.data else {
        return Err(Error::new(input.ident.span(), "an enum"));
    };
    if !input.generics.params.is_empty() {
        return Err(Error::new(input.generics.span(), "an enum of no generics"));
    }
    data.variants
        .iter()
        .map(|variant| match variant.fields {
            Fields::Unit => Ok(variant),
            Fields::Named(_) | Fields::Unnamed(_) => {
                Err(Error::new(variant.span(), "a variant of no data"))
            }
        })
        .collect()
}

/// `ident` in snake case, as serde's `rename_all = "snake_case"` writes a variant: an underscore
/// before each capital past the first, every letter lower case.
fn snake(ident: &Ident) -> String {
    let mut snake = String::new();
    for (at, letter) in ident.unraw().to_string().char_indices() {
        if at > 0 && letter.is_uppercase() {
            snake.push('_');
        }
        snake.push(letter.to_ascii_lowercase());
    }
    snake
}

/// The name `#[name = "<name>"]` gives `variant`, if any, once.
fn name_of(variant: &Variant) -> syn::Result<Option<LitStr>> {
    let mut found = None;
    for attr in variant
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("name"))
    {
        let Meta::NameValue(pair) = &attr.meta else {
            return Err(Error::new(attr.span(), "`#[name = \"<name>\"]`"));
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(name),
            ..
        }) = &pair.value
        else {
            return Err(Error::new(pair.value.span(), "a name in quotes"));
        };
        if name.value().is_empty() {
            return Err(Error::new(name.span(), "a name of at least one letter"));
        }
        if found.replace(name.clone()).is_some() {
            return Err(Error::new(attr.span(), "a second name"));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    #[test]
    fn snake_case_puts_an_underscore_before_each_capital_past_the_first() {
        let snaked = [
            parse_quote!(Friendly),
            parse_quote!(HoldAtInputs),
            parse_quote!(AIUpdate),
            parse_quote!(r#Type),
        ]
        .map(|ident: Ident| snake(&ident));
        assert_eq!(snaked, ["friendly", "hold_at_inputs", "a_i_update", "type"]);
    }

    #[test]
    fn a_named_enum_refuses_an_empty_name_two_of_one_name_a_variant_of_data_and_a_discriminant() {
        let refused = |input: DeriveInput| named_enum(&input).err().map(|error| error.to_string());
        assert_eq!(
            refused(parse_quote!(
                enum Side {
                    North,
                    #[name = "north"]
                    South,
                }
            )),
            Some("a second member of this name".to_owned())
        );
        assert_eq!(
            refused(parse_quote!(
                enum Side {
                    #[name = ""]
                    North,
                }
            )),
            Some("a name of at least one letter".to_owned())
        );
        assert_eq!(
            refused(parse_quote!(
                enum Side {
                    North(u8),
                }
            )),
            Some("a variant of no data".to_owned())
        );
        assert_eq!(
            refused(parse_quote!(
                struct Side;
            )),
            Some("an enum".to_owned())
        );
        assert_eq!(
            refused(parse_quote!(
                enum Side {
                    North,
                    South = 4,
                }
            )),
            Some(
                "a member of no discriminant of its own, so its place is its discriminant"
                    .to_owned()
            )
        );
        let named = parse_quote!(
            enum Side {
                North,
                #[name = "0_2"]
                ZeroTwo,
            }
        );
        assert_eq!(refused(named), None);
    }
}
