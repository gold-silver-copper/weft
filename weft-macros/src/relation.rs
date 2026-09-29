//! `#[relation(Source -> Target, arity, loop rule, policy, on_relate = f: Bound)]`.

use proc_macro2::{Span, TokenStream, TokenTree};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Ident, ItemStruct, Path, Token, Type};

struct Declaration {
    source: Type,
    target: Type,
    two_way: Option<Span>,
    arity: Option<(Ident, Arity)>,
    loops: Option<(Ident, Loops)>,
    policy: Option<(Ident, Policy)>,
    hook: Option<(Path, Punctuated<Path, Token![+]>)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arity {
    One,
    Opt,
    Many,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Loops {
    Ranked,
    Cyclic,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    Cascade,
    Detach,
    Spill,
    Refuse,
}

/// Splits `Source -> Target, rest` or `Source <-> Target, rest` at the arrow and the first
/// top-level comma. The endpoints may be generic types, so `<` alone isn't an arrow.
fn split(attr: TokenStream) -> syn::Result<(TokenStream, Option<Span>, Span, TokenStream, TokenStream)> {
    let tokens: Vec<TokenTree> = attr.into_iter().collect();
    let is = |i: usize, c: char| matches!(tokens.get(i), Some(TokenTree::Punct(p)) if p.as_char() == c);
    let joint = |i: usize| matches!(tokens.get(i), Some(TokenTree::Punct(p)) if p.spacing() == proc_macro2::Spacing::Joint);
    let arrow = (0..tokens.len()).find(|&i| is(i, '-') && joint(i) && is(i.saturating_add(1), '>'));
    let Some(dash) = arrow else {
        return Err(syn::Error::new(
            Span::call_site(),
            "expected `Source -> Target` or `Source <-> Target`, then the options: `#[relation(Thing -> Holder, one, cascade)]`",
        ));
    };
    let two_way = dash.checked_sub(1).filter(|&lt| is(lt, '<') && joint(lt));
    let source_end = two_way.unwrap_or(dash);
    let arrow_span = tokens.get(source_end).map_or_else(Span::call_site, TokenTree::span);
    let mut depth = 0_i32;
    let after = dash.saturating_add(2);
    let comma = (after..tokens.len()).find(|&i| {
        if is(i, '<') {
            depth = depth.saturating_add(1);
        } else if is(i, '>') {
            depth = depth.saturating_sub(1);
        }
        depth == 0 && is(i, ',')
    });
    let target_end = comma.unwrap_or(tokens.len());
    let collect = |from: usize, to: usize| -> TokenStream { tokens.get(from..to).unwrap_or(&[]).iter().cloned().collect() };
    Ok((
        collect(0, source_end),
        two_way.map(|_| arrow_span),
        arrow_span,
        collect(after, target_end),
        comma.map_or_else(TokenStream::new, |c| collect(c.saturating_add(1), tokens.len())),
    ))
}

enum Option_ {
    Word(Ident),
    Hook(Path, Punctuated<Path, Token![+]>),
}

impl Parse for Option_ {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let word: Ident = input.parse()?;
        if word == "on_relate" {
            input.parse::<Token![=]>()?;
            let hook: Path = input.parse()?;
            input.parse::<Token![:]>()?;
            let bounds = Punctuated::<Path, Token![+]>::parse_separated_nonempty(input)?;
            Ok(Option_::Hook(hook, bounds))
        } else {
            Ok(Option_::Word(word))
        }
    }
}

fn set<T>(slot: &mut Option<(Ident, T)>, word: Ident, value: T, what: &str) -> syn::Result<()> {
    if let Some((first, _)) = slot {
        return Err(syn::Error::new(word.span(), format!("a relation has one {what}, and this one already says `{first}`")));
    }
    *slot = Some((word, value));
    Ok(())
}

fn parse(attr: TokenStream) -> syn::Result<Declaration> {
    let (source, two_way, arrow, target, rest) = split(attr)?;
    if source.is_empty() {
        return Err(syn::Error::new(arrow, "the relation's source is missing before the arrow"));
    }
    if target.is_empty() {
        return Err(syn::Error::new(arrow, "the relation's target is missing after the arrow"));
    }
    let mut declaration = Declaration {
        source: syn::parse2(source)?,
        target: syn::parse2(target)?,
        two_way,
        arity: None,
        loops: None,
        policy: None,
        hook: None,
    };
    let options = syn::parse::Parser::parse2(Punctuated::<Option_, Token![,]>::parse_terminated, rest)?;
    for option in options {
        let word = match option {
            Option_::Hook(hook, bounds) => {
                if declaration.hook.is_some() {
                    return Err(syn::Error::new(hook.span(), "a relation has at most one `on_relate` hook"));
                }
                declaration.hook = Some((hook, bounds));
                continue;
            }
            Option_::Word(word) => word,
        };
        let text = word.to_string();
        match text.as_str() {
            "one" => set(&mut declaration.arity, word, Arity::One, "arity")?,
            "opt" => set(&mut declaration.arity, word, Arity::Opt, "arity")?,
            "many" => set(&mut declaration.arity, word, Arity::Many, "arity")?,
            "ranked" => set(&mut declaration.loops, word, Loops::Ranked, "loop rule")?,
            "cyclic" => set(&mut declaration.loops, word, Loops::Cyclic, "loop rule")?,
            "cascade" => set(&mut declaration.policy, word, Policy::Cascade, "despawn policy")?,
            "detach" => set(&mut declaration.policy, word, Policy::Detach, "despawn policy")?,
            "spill" => set(&mut declaration.policy, word, Policy::Spill, "despawn policy")?,
            "refuse" => set(&mut declaration.policy, word, Policy::Refuse, "despawn policy")?,
            _ => {
                return Err(syn::Error::new(
                    word.span(),
                    format!(
                        "unknown option `{text}`: expected an arity (`one`, `opt`, `many`), a loop rule (`ranked`, `cyclic`), \
                         a despawn policy (`cascade`, `detach`, `spill`, `refuse`) or `on_relate = f: Bound`"
                    ),
                ));
            }
        }
    }
    Ok(declaration)
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut item: ItemStruct = syn::parse2(item)?;
    let d = parse(attr)?;
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new(item.generics.span(), "a relation can't be generic"));
    }
    let Some((arity_word, arity)) = &d.arity else {
        return Err(syn::Error::new(item.ident.span(), "say how many targets a source has: `one`, `opt` or `many`"));
    };
    let Some((policy_word, policy)) = &d.policy else {
        return Err(syn::Error::new(
            item.ident.span(),
            "say what despawning a target does: `cascade`, `detach`, `spill` or `refuse`",
        ));
    };
    // Rule violations are reported, and the relation is still expanded as declared, so the
    // one mistake doesn't bury itself under errors from everything that uses the relation.
    let mut problems: Option<syn::Error> = None;
    let mut problem = |span: Span, message: String| {
        let e = syn::Error::new(span, message);
        match &mut problems {
            Some(all) => all.combine(e),
            None => problems = Some(e),
        }
    };
    match (arity, policy) {
        (Arity::One, Policy::Detach) => problem(
            policy_word.span(),
            "`detach` would leave a `one` edge's source with no edge: use `cascade`, `spill` or `refuse`".to_owned(),
        ),
        (Arity::Opt | Arity::Many, Policy::Spill) => problem(
            policy_word.span(),
            "`spill` moves the source's `one` edge up a level, so it needs a `one` relation".to_owned(),
        ),
        _ => {}
    }
    if let Some(arrow) = d.two_way {
        let text = |t: &Type| quote!(#t).to_string();
        if text(&d.source) != text(&d.target) {
            return Err(syn::Error::new(arrow, "a two-way (`<->`) relation joins a kind or set to itself: `Room <-> Room`"));
        }
        if let Arity::One | Arity::Opt = arity {
            problem(arrow, format!("a two-way (`<->`) relation must be `many`, not `{arity_word}`"));
        }
    }
    let problems = problems.map(syn::Error::into_compile_error);
    crate::kind::derive_debug(&mut item)?;

    let name = &item.ident;
    let name_text = name.to_string();
    let (source, target) = (&d.source, &d.target);
    let p = quote!(::weft::__private);
    let arity_ty = match arity {
        Arity::One => quote!(::weft::One),
        Arity::Opt => quote!(::weft::Opt),
        Arity::Many => quote!(::weft::Many),
    };
    let loops_ty = match d.loops.as_ref().map(|(_, l)| l) {
        None => quote!(::weft::NoLoops),
        Some(Loops::Ranked) => quote!(::weft::Ranked),
        Some(Loops::Cyclic) => quote!(::weft::Cyclic),
    };
    let policy_ty = match policy {
        Policy::Cascade => quote!(::weft::Cascade),
        Policy::Detach => quote!(::weft::Detach),
        Policy::Spill => quote!(::weft::Spill),
        Policy::Refuse => quote!(::weft::Refuse),
    };
    let two_way = d.two_way.is_some();
    let (vals, fwd, back, targets, sources, value) = if two_way {
        (
            quote!((Self, Self)),
            quote!(&vals.0),
            quote!(&vals.1),
            quote!(#p::two_way(log, at)),
            quote!(#p::two_way(log, at)),
            quote!(impl ::weft::TwoWay for #name {}),
        )
    } else {
        (
            quote!(Self),
            quote!(vals),
            quote!(vals),
            quote!(#p::one_way_targets(log, at)),
            quote!(#p::one_way_sources(log, at)),
            quote! {
                impl ::weft::Value for #name {
                    type Relation = #name;
                    fn vals(self) -> Self {
                        self
                    }
                }
            },
        )
    };
    let (hook_ty, on_relate) = match &d.hook {
        None => (
            quote!(::weft::Sure),
            quote! {
                impl<G: ::weft::Schema> ::weft::OnRelate<G> for #name {
                    fn fire<'t>(
                        _: &mut ::weft::Turn<'t, G, ::weft::Live>,
                        _: ::weft::Id<'t, #source>,
                        _: ::weft::Id<'t, #target>,
                    ) -> ::core::result::Result<(), ::core::convert::Infallible> {
                        ::core::result::Result::Ok(())
                    }
                }
            },
        ),
        Some((hook, bounds)) => (
            quote!(::weft::Refused),
            quote! {
                impl<G: #bounds> ::weft::OnRelate<G> for #name {
                    fn fire<'t>(
                        t: &mut ::weft::Turn<'t, G, ::weft::Live>,
                        source: ::weft::Id<'t, #source>,
                        target: ::weft::Id<'t, #target>,
                    ) -> ::core::result::Result<(), ::weft::Refused> {
                        #hook(t, source, target)
                    }
                }
            },
        ),
    };
    let link = match arity {
        Arity::One => quote! {
            impl<X: #p::Carries<#name> + ::weft::Member<#source>, Y: ::weft::Member<#target>> #p::Link<X, Y> for #name {
                fn link<'t>(log: &'t #p::RelLog<Self>, s: ::weft::Id<'t, X>, value: Self, d: ::weft::Id<'t, Y>) {
                    #p::link_one(log, s, value, d)
                }
            }
            impl<'t, K: #p::Carries<#name>> ::weft::Holds<'t, #name> for ::weft::Id<'t, K> {
                fn read(self, _: &'t #p::RelLog<#name>) -> ::weft::Id<'t, #target> {
                    #p::read_one::<#name, K>(self)
                }
            }
        },
        Arity::Opt => quote! {
            impl<X: ::weft::Member<#source>, Y: ::weft::Member<#target>> #p::Link<X, Y> for #name {
                fn link<'t>(log: &'t #p::RelLog<Self>, s: ::weft::Id<'t, X>, vals: <Self as ::weft::Relation>::Vals, d: ::weft::Id<'t, Y>) {
                    #p::link_opt(log, s, vals, d)
                }
            }
            impl<'t, K: ::weft::Kind + ::weft::Member<#source>> ::weft::Holds<'t, #name> for ::weft::Id<'t, K> {
                fn read(self, log: &'t #p::RelLog<#name>) -> ::core::option::Option<::weft::Id<'t, #target>> {
                    #p::read_opt::<#name, K>(self, log)
                }
            }
            impl ::weft::Detachable for #name {}
        },
        Arity::Many => quote! {
            impl<X: ::weft::Member<#source>, Y: ::weft::Member<#target>> #p::Link<X, Y> for #name {
                fn link<'t>(log: &'t #p::RelLog<Self>, s: ::weft::Id<'t, X>, vals: <Self as ::weft::Relation>::Vals, d: ::weft::Id<'t, Y>) {
                    #p::link_many(log, s, vals, d)
                }
            }
            impl ::weft::Detachable for #name {}
        },
    };

    Ok(quote! {
        #problems
        #item
        impl #p::Part for #name {
            type Side = #p::Shared;
            type Store = #p::RelLog<Self>;
        }
        impl ::weft::Relation for #name {
            type Source = #source;
            type Target = #target;
            type Arity = #arity_ty;
            type Loops = #loops_ty;
            type Policy = #policy_ty;
            type Vals = #vals;
            type Hook = #hook_ty;
            const NAME: &'static str = #name_text;
            const TWO_WAY: bool = #two_way;
            fn fwd(vals: &Self::Vals) -> &Self {
                #fwd
            }
            fn back(vals: &Self::Vals) -> &Self {
                #back
            }
            fn targets_of<'t>(log: &'t #p::RelLog<Self>, at: usize) -> #p::Vec<(::weft::Id<'t, #target>, &'t Self)> {
                #targets
            }
            fn sources_of<'t>(log: &'t #p::RelLog<Self>, at: usize) -> #p::Vec<(::weft::Id<'t, #source>, &'t Self)> {
                #sources
            }
        }
        #value
        #on_relate
        #link
    })
}
