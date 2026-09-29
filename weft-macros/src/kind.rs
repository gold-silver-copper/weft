//! `#[kind]` and `#[component]`.

use darling::FromMeta;
use darling::ast::NestedMeta;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Expr, GenericParam, ItemStruct, Lit, Path};

/// The most `needs` a kind can have: the library implements node links and spawn arguments
/// up to this many.
const MAX_NEEDS: usize = 8;

/// The highest numeric rank. Each step is a type, `Up<..>`.
const MAX_RANK: u16 = 255;

/// `needs = [In, Plays]`, `is = [Thing, Holder]`, or a single name without the brackets.
#[derive(Default)]
struct Names(Vec<Path>);

impl FromMeta for Names {
    fn from_expr(expr: &Expr) -> darling::Result<Self> {
        match expr {
            Expr::Array(array) => array.elems.iter().map(name).collect::<darling::Result<_>>().map(Names),
            Expr::Group(group) => Self::from_expr(&group.expr),
            single => name(single).map(|path| Names(vec![path])),
        }
    }
}

fn name(expr: &Expr) -> darling::Result<Path> {
    match expr {
        Expr::Path(path) if path.qself.is_none() => Ok(path.path.clone()),
        Expr::Group(group) => name(&group.expr),
        other => Err(darling::Error::custom("expected the name of a relation or a set").with_span(other)),
    }
}

/// `rank = 3`, `rank = R3`, or `rank = R` for the struct's own parameter.
enum Rank {
    Number(u16, Span),
    Type(Path),
}

impl FromMeta for Rank {
    fn from_expr(expr: &Expr) -> darling::Result<Self> {
        match expr {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Int(int) => int
                    .base10_parse::<u16>()
                    .map(|n| Rank::Number(n, int.span()))
                    .map_err(|_| darling::Error::custom("a rank is a number from 0 to 255").with_span(int)),
                other => Err(darling::Error::custom("a rank is a number or a rank type").with_span(other)),
            },
            Expr::Path(path) if path.qself.is_none() => Ok(Rank::Type(path.path.clone())),
            Expr::Group(group) => Self::from_expr(&group.expr),
            other => Err(darling::Error::custom("a rank is a number or a rank type").with_span(other)),
        }
    }
}

/// `#[kind(..)]`'s arguments. In a module of its own so the `allow` below covers the impl that
/// darling's derive generates, which contains an `unreachable!`.
mod args {
    #![allow(clippy::unreachable)]

    use darling::FromMeta;

    use super::{Names, Rank};

    #[derive(Default, FromMeta)]
    pub(super) struct KindArgs {
        #[darling(default)]
        pub(super) root: bool,
        #[darling(default)]
        pub(super) needs: Names,
        #[darling(default)]
        pub(super) rank: Option<Rank>,
        #[darling(default)]
        pub(super) is: Names,
    }
}

use args::KindArgs;

/// darling's errors, as one `syn` error that keeps every message and span.
fn darling_error(error: darling::Error) -> syn::Error {
    let mut all: Option<syn::Error> = None;
    for e in error {
        let e = syn::Error::new(e.span(), e.to_string());
        match &mut all {
            Some(all) => all.combine(e),
            None => all = Some(e),
        }
    }
    all.unwrap_or_else(|| syn::Error::new(Span::call_site(), "invalid `#[kind]` arguments"))
}

/// Adds `#[derive(Debug)]` unless the item already derives `Debug`.
pub(crate) fn derive_debug(item: &mut ItemStruct) -> syn::Result<()> {
    for attr in item.attrs.iter().filter(|a| a.path().is_ident("derive")) {
        let mut derives = false;
        attr.parse_nested_meta(|meta| {
            derives |= meta.path.segments.last().is_some_and(|s| s.ident == "Debug");
            Ok(())
        })?;
        if derives {
            return Ok(());
        }
    }
    item.attrs.push(syn::parse_quote!(#[derive(Debug)]));
    Ok(())
}

fn rank_type(n: u16) -> TokenStream {
    let mut ty = quote!(::weft::Zero);
    for _ in 0..n {
        ty = quote!(::weft::Up<#ty>);
    }
    ty
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut item: ItemStruct = syn::parse2(item)?;
    let args = NestedMeta::parse_meta_list(attr)?;
    let args = KindArgs::from_list(&args).map_err(darling_error)?;
    derive_debug(&mut item)?;

    // A generic kind has exactly one type parameter, and it's the rank.
    let param = match item.generics.params.iter().collect::<Vec<_>>().as_slice() {
        [] => None,
        [GenericParam::Type(param)] if param.bounds.is_empty() && item.generics.where_clause.is_none() => Some(param.ident.clone()),
        _ => {
            return Err(syn::Error::new(
                item.generics.span(),
                "a kind can be generic only over its rank: `#[kind(rank = R)] struct Chest<R> { .., rank: R }`",
            ));
        }
    };
    let rank = match (&args.rank, &param) {
        (_, Some(param)) if args.root => return Err(syn::Error::new(param.span(), "the root kind can't be generic")),
        (Some(_), None) if args.root => return Err(syn::Error::new(item.ident.span(), "the root kind has no rank")),
        (Some(Rank::Type(path)), Some(param)) if path.is_ident(param) => quote!(#param),
        (_, Some(param)) => {
            return Err(syn::Error::new(param.span(), format!("a generic kind takes its rank from its parameter: `#[kind(rank = {param})]`")));
        }
        (Some(Rank::Number(n, span)), None) if *n > MAX_RANK => {
            return Err(syn::Error::new(*span, format!("ranks run from 0 to {MAX_RANK}")));
        }
        (Some(Rank::Number(n, _)), None) => rank_type(*n),
        (Some(Rank::Type(path)), None) => quote!(#path),
        (None, None) => quote!(::weft::Top),
    };
    if let Some(extra) = args.needs.0.get(MAX_NEEDS) {
        return Err(syn::Error::new(extra.span(), format!("a kind can need at most {MAX_NEEDS} relations")));
    }

    let name = &item.ident;
    let name_text = name.to_string();
    let (generics, self_ty) = match &param {
        Some(p) => (quote!(<#p: ::weft::Rank>), quote!(#name<#p>)),
        None => (quote!(), quote!(#name)),
    };
    let (role, payload) =
        if args.root { (quote!(::weft::__private::RootRole), quote!(())) } else { (quote!(::weft::__private::Plural), quote!(Self)) };
    let write_param = param.as_ref().map(|p| quote!(::core::write!(f, "<{}>", <#p as ::weft::Rank>::NUM)?;));
    let needs = &args.needs.0;
    let slots = crate::each(needs.iter().map(|n| quote!(::weft::__private::Rc<::weft::__private::OneSlot<#n>>)), quote!(,));
    let carries: TokenStream = needs.iter().enumerate().map(|(i, n)| {
        let i = syn::Index::from(i);
        quote! {
            impl #generics ::weft::__private::Carries<#n> for #self_ty {
                fn slot(links: &<Self as ::weft::Kind>::Links) -> &::weft::__private::Rc<::weft::__private::OneSlot<#n>> {
                    &links.#i
                }
            }
        }
    }).collect();
    let one_only: TokenStream = needs.iter().map(|n| {
        let message = format!("`needs` takes `one` relations, and `{}` isn't one", quote!(#n));
        quote_spanned! {n.span()=>
            const _: () = ::core::assert!(<<#n as ::weft::Relation>::Arity as ::weft::__private::Arity>::ONE, #message);
        }
    }).collect();
    let sets: TokenStream = args.is.0.iter().map(|set| {
        quote! {
            impl #generics ::weft::Member<#set> for #self_ty {
                fn up<'t>(r: <Self as ::weft::Handle>::Ref<'t>) -> &'t ::weft::__private::Dyn {
                    r
                }
                fn own(r: <Self as ::weft::Handle>::Ref<'_>) -> ::weft::__private::Box<::weft::__private::Dyn> {
                    ::weft::__private::Erased::boxed(r)
                }
            }
        }
    }).collect();

    Ok(quote! {
        #item
        impl #generics ::weft::__private::Part for #self_ty {
            type Side = ::weft::__private::Shared;
            type Store = ::weft::__private::KindList<Self>;
        }
        impl #generics ::weft::Kind for #self_ty {
            type Rank = #rank;
            type Role = #role;
            type Payload = #payload;
            type Links = (#slots);
            fn write_name(f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(#name_text)?;
                #write_param
                ::core::result::Result::Ok(())
            }
        }
        #one_only
        #carries
        #sets
    })
}

pub(crate) fn component(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut item: ItemStruct = syn::parse2(item)?;
    if attr.is_empty() {
        return Err(syn::Error::new(item.ident.span(), "say which kind has it: `#[component(Item)]`"));
    }
    let kind: syn::Type = syn::parse2(attr)?;
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new(item.generics.span(), "a component can't be generic"));
    }
    derive_debug(&mut item)?;
    let name = &item.ident;
    Ok(quote! {
        #item
        impl ::weft::__private::Part for #name {
            type Side = ::weft::__private::Owned;
            type Store = ::weft::__private::Table<Self>;
        }
        impl ::weft::Component for #name {
            type Kind = #kind;
        }
    })
}
