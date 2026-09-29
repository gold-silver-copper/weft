//! `module!`, `game!`, and the step between them.
//!
//! `game!` can't see into a module from another crate, so each module defines a macro (a
//! `decl_macro`, so it has a path) that adds its parts to the list being built and hands it
//! on. `game!` calls the first module's macro; each calls `__game_step`, which calls the next
//! one, and after the last, emits the game. The parts' tokens keep the spans of the module
//! that listed them, so they resolve where that module was declared.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote, quote_spanned};

use crate::{each, join};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Ident, Path, Token, Type, Visibility, bracketed, braced};

/// A comma-separated list of types, trailing comma allowed.
fn types(input: ParseStream<'_>) -> syn::Result<Vec<Type>> {
    Ok(Punctuated::<Type, Token![,]>::parse_terminated(input)?.into_iter().collect())
}

struct Module {
    vis: Visibility,
    name: Ident,
    kinds: Vec<Type>,
    relations: Vec<Type>,
    components: Vec<Type>,
}

impl Parse for Module {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let vis = input.parse()?;
        let name = input.parse()?;
        let body;
        braced!(body in input);
        let mut module = Module { vis, name, kinds: Vec::new(), relations: Vec::new(), components: Vec::new() };
        let mut seen = BTreeSet::new();
        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![:]>()?;
            let list;
            bracketed!(list in body);
            let parts = types(&list)?;
            let text = key.to_string();
            if !seen.insert(text.clone()) {
                return Err(syn::Error::new(key.span(), format!("`{text}` is listed twice")));
            }
            match text.as_str() {
                "kinds" => module.kinds = parts,
                "relations" => module.relations = parts,
                "components" => module.components = parts,
                _ => return Err(syn::Error::new(key.span(), "expected `kinds`, `relations` or `components`")),
            }
            if !body.is_empty() {
                body.parse::<Token![,]>()?;
            }
        }
        Ok(module)
    }
}

pub(crate) fn module(input: TokenStream) -> syn::Result<TokenStream> {
    let Module { vis, name, kinds, relations, components } = syn::parse2(input)?;
    let bounds: TokenStream = kinds.iter().chain(&relations).chain(&components).map(|t| quote!(+ ::weft::Has<#t>)).collect();
    let (kinds, relations, components) = (each(&kinds, quote!(,)), each(&relations, quote!(,)), each(&components, quote!(,)));
    let d = proc_macro2::Punct::new('$', proc_macro2::Spacing::Alone);
    Ok(quote! {
        #vis trait #name: ::weft::Schema #bounds {}
        impl<G: ::weft::Schema #bounds> #name for G {}
        #vis macro #name(@collect #d h:tt #d rest:tt [#d(#d ak:ty,)*] [#d(#d ar:ty,)*] [#d(#d ac:ty,)*]) {
            ::weft::__private::game_step! {
                @next #d h #d rest
                [#d(#d ak,)* #kinds]
                [#d(#d ar,)* #relations]
                [#d(#d ac,)* #components]
            }
        }
    })
}

struct Header {
    vis: Visibility,
    name: Ident,
    root: Type,
}

impl Parse for Header {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let vis = input.parse()?;
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let root = input.parse()?;
        Ok(Header { vis, name, root })
    }
}

impl ToTokens for Header {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Header { vis, name, root } = self;
        tokens.extend(quote!(#vis #name: #root));
    }
}

/// `pub Game: Settings [Map, other::Containers]`
struct Game {
    header: Header,
    modules: Vec<Path>,
}

impl Parse for Game {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let header = input.parse()?;
        let list;
        bracketed!(list in input);
        let modules = Punctuated::<Path, Token![,]>::parse_terminated(&list)?.into_iter().collect();
        Ok(Game { header, modules })
    }
}

pub(crate) fn game(input: TokenStream) -> syn::Result<TokenStream> {
    let Game { header, modules } = syn::parse2(input)?;
    match modules.split_first() {
        None => Ok(emit(&header, &[], &[], &[])),
        Some((first, rest)) => {
            let rest = join(rest, quote!(,));
            Ok(quote!(#first! { @collect [#header] [#rest] [] [] [] }))
        }
    }
}

/// `@next [header] [modules left] [kinds] [relations] [components]`
struct Step {
    header: Header,
    rest: Vec<Path>,
    kinds: Vec<Type>,
    relations: Vec<Type>,
    components: Vec<Type>,
}

impl Parse for Step {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![@]>()?;
        let next: Ident = input.parse()?;
        if next != "next" {
            return Err(syn::Error::new(next.span(), "expected `@next`"));
        }
        let group = |input: ParseStream<'_>| -> syn::Result<TokenStream> {
            let content;
            bracketed!(content in input);
            content.parse()
        };
        let header = syn::parse2(group(input)?)?;
        let rest = Punctuated::<Path, Token![,]>::parse_terminated.parse2(group(input)?)?.into_iter().collect();
        let kinds = types.parse2(group(input)?)?;
        let relations = types.parse2(group(input)?)?;
        let components = types.parse2(group(input)?)?;
        Ok(Step { header, rest, kinds, relations, components })
    }
}

pub(crate) fn step(input: TokenStream) -> syn::Result<TokenStream> {
    let Step { header, rest, kinds, relations, components } = syn::parse2(input)?;
    match rest.split_first() {
        Some((next, rest)) => {
            let rest = join(rest, quote!(,));
            let (kinds, relations, components) = (each(&kinds, quote!(,)), each(&relations, quote!(,)), each(&components, quote!(,)));
            Ok(quote!(#next! { @collect [#header] [#rest] [#kinds] [#relations] [#components] }))
        }
        None => Ok(emit(&header, &kinds, &relations, &components)),
    }
}

/// A type as the checks name it: `Chest<R1>`, not `Chest < R1 >`.
fn pretty(ty: &Type) -> String {
    ty.to_token_stream().to_string().replace(" <", "<").replace("< ", "<").replace(" >", ">").replace(" ,", ",").replace(" :: ", "::")
}

/// The same part listed by two modules is stored once.
fn dedup(parts: &[Type]) -> Vec<&Type> {
    let mut seen = BTreeSet::new();
    parts.iter().filter(|t| seen.insert(t.to_token_stream().to_string())).collect()
}

fn emit(header: &Header, kinds: &[Type], relations: &[Type], components: &[Type]) -> TokenStream {
    let Header { vis, name, root } = header;
    let span = name.span();
    let (kinds, relations, components) = (dedup(kinds), dedup(relations), dedup(components));
    let p = quote_spanned!(span=> ::weft::__private);

    // (KindList<Root>, (Store<kind>.., (Store<relation>.., ())))
    let graph: Vec<&Type> = kinds.iter().chain(&relations).copied().collect();
    let shared = graph.iter().rev().fold(quote_spanned!(span=> ()), |tail, part| {
        quote_spanned!(span=> (<#part as #p::Part>::Store, #tail))
    });
    let comps = components.iter().rev().fold(quote_spanned!(span=> ()), |tail, part| {
        quote_spanned!(span=> (<#part as #p::Part>::Store, #tail))
    });
    let has = |parts: &[&Type], skip_root: bool| -> Vec<TokenStream> {
        parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let depth = if skip_root { i.saturating_add(1) } else { i };
                let path: TokenStream = (0..depth).map(|_| quote!(.1)).collect();
                quote_spanned! {span=>
                    impl ::weft::Has<#part> for #name {
                        fn part(s: &<<#part as #p::Part>::Side as #p::Side>::Of<Self>) -> &<#part as #p::Part>::Store {
                            &s #path .0
                        }
                        fn part_mut(s: &mut <<#part as #p::Part>::Side as #p::Side>::Of<Self>) -> &mut <#part as #p::Part>::Store {
                            &mut s #path .0
                        }
                    }
                }
            })
            .collect()
    };
    let has_graph: TokenStream = has(&graph, true).into_iter().collect();
    let has_comps: TokenStream = has(&components, false).into_iter().collect();

    // The declaration checks: a const per question, failing `cargo check` with a message.
    let every_kind: Vec<&Type> = std::iter::once(root).chain(kinds.iter().copied()).collect();
    let mut checks = TokenStream::new();
    for kind in &every_kind {
        for relation in &relations {
            let (k, r) = (pretty(kind), pretty(relation));
            let unneeded = format!(
                "`{k}` can be the source of the `one` relation `{r}`, so it must need it: `#[kind(needs = [{r}])]`"
            );
            let overlap = format!(
                "`{k}` can be both the source and the target of `{r}`, so `{r}` needs a loop rule: `ranked` or `cyclic`"
            );
            checks.extend(quote_spanned! {span=>
                const _: () = {
                    #[allow(unused_imports)]
                    use #p::Otherwise as _;
                    if <#p::Probe<#kind, #relation>>::ONE_SOURCE && !<#p::Probe<#kind, #relation>>::NEEDED {
                        ::core::panic!(#unneeded);
                    }
                    if <#p::Probe<#kind, #relation>>::LOOPLESS_OVERLAP {
                        ::core::panic!(#overlap);
                    }
                };
            });
        }
    }
    for relation in &relations {
        let message = format!(
            "the root `{}` can be the source of `{}`, which may not `cascade` or `spill`: the root is never despawned or moved",
            pretty(root),
            pretty(relation)
        );
        checks.extend(quote_spanned! {span=>
            const _: () = {
                #[allow(unused_imports)]
                use #p::Otherwise as _;
                if <#p::Probe<#root, #relation>>::TAKES {
                    ::core::panic!(#message);
                }
            };
        });
    }
    let owners: Vec<String> = every_kind
        .iter()
        .map(|k| {
            format!(
                "through `one` edges, a `{}` could own itself: make one of the relations on that loop `opt` or `many`, \
                 or `ranked` with rising ranks",
                pretty(k)
            )
        })
        .collect();
    let owners = join(owners, quote!(,));
    let rows = join(
        every_kind.iter().map(|a| {
            let cells = join(
                every_kind.iter().map(|b| {
                    let probes: TokenStream =
                        relations.iter().map(|r| quote_spanned!(span=> || <#p::Probe<#a, #r, #b>>::OWNS)).collect();
                    quote_spanned!(span=> false #probes)
                }),
                quote!(,),
            );
            quote_spanned!(span=> &[#cells])
        }),
        quote!(,),
    );
    checks.extend(quote_spanned! {span=>
        const _: () = {
            #[allow(unused_imports)]
            use #p::Otherwise as _;
            const OWNERS: &[&str] = &[#owners];
            const OWNS: &[&[bool]] = &[#rows];
            if let ::core::option::Option::Some(i) = #p::owns_itself(OWNS) {
                ::core::panic!("{}", #p::nth(OWNERS, i));
            }
        };
    });

    quote_spanned! {span=>
        #vis struct #name;
        impl ::weft::Schema for #name {
            type Root = #root;
            type Shared = (#p::KindList<#root>, #shared);
            type Comps = #comps;
            fn root_list(shared: &Self::Shared) -> &#p::KindList<#root> {
                &shared.0
            }
        }
        #has_graph
        #has_comps
        #checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_types() {
        let ty: Type = syn::parse_quote!(containers::Chest<R1>);
        assert_eq!(pretty(&ty), "containers::Chest<R1>");
    }

    #[test]
    fn a_module_parses() {
        let module: Module = syn::parse_quote!(pub Map { kinds: [Room, Chest<R1>], relations: [In,], });
        assert_eq!(module.kinds.len(), 2);
        assert_eq!(module.relations.len(), 1);
        assert!(module.components.is_empty());
    }
}
