//! The declaration macros.

/// Declares an entity kind for a struct.
///
/// ```text
/// kind!(Room);                              // any number per world, rank 255
/// kind!(Item rank 0 needs In);              // spawned only with its `In` edge
/// kind!(Chest<const R: u8> rank R needs In); // one kind per rank
/// kind!(root Settings needs Start, Plays);  // the root: one per world, its data is the settings
/// ```
///
/// `needs` lists the kind's `one` relations, in the order `spawn` takes their edges. Every
/// kind that can be the source of a `one` relation must need it (`game!` checks).
#[macro_export]
macro_rules! kind {
    (root $k:ident $(needs $($n:ident),+)?) => {
        $crate::__kind!([] [$k] [] $k [255] $crate::__private::RootRole, (); $($($n),+)?);
    };
    ($k:ident < const $p:ident : $pt:ty > $(rank $r:tt)? $(needs $($n:ident),+)?) => {
        $crate::__kind!([const $p: $pt] [$k<$p>] [$p] $k [$crate::__rank!($($r)?)] $crate::__private::Plural, Self; $($($n),+)?);
    };
    ($k:ident $(rank $r:tt)? $(needs $($n:ident),+)?) => {
        $crate::__kind!([] [$k] [] $k [$crate::__rank!($($r)?)] $crate::__private::Plural, Self; $($($n),+)?);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __rank {
    () => {
        255
    };
    ($r:tt) => {
        $r
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __kind {
    ([$($g:tt)*] [$($ty:tt)*] [$($p:ident)?] $name:ident [$($rank:tt)*] $role:path, $payload:ty; $($n:ident),*) => {
        impl<$($g)*> $crate::__private::Part for $($ty)* {
            type Side = $crate::__private::Shared;
            type Store = $crate::__private::KindList<Self>;
        }
        impl<$($g)*> $crate::Kind for $($ty)* {
            const RANK: u8 = $($rank)*;
            type Role = $role;
            type Payload = $payload;
            type Links = ($($crate::__private::Rc<$crate::__private::OneSlot<$n>>,)*);
            fn write_name(f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(::core::stringify!($name))?;
                $(::core::write!(f, "<{}>", $p)?;)?
                ::core::result::Result::Ok(())
            }
        }
        $(
            const _: () = ::core::assert!(
                <<$n as $crate::Relation>::Arity as $crate::__private::Arity>::ONE,
                ::core::concat!("`needs` takes `one` relations, and `", ::core::stringify!($n), "` isn't one"),
            );
        )*
        $crate::__carries!([$($g)*] [$($ty)*] [0 1 2 3] $($n),*);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __carries {
    ([$($g:tt)*] [$($ty:tt)*] [$($is:tt)*]) => {};
    ([$($g:tt)*] [$($ty:tt)*] [$i:tt $($is:tt)*] $n:ident $(, $rest:ident)*) => {
        impl<$($g)*> $crate::__private::Carries<$n> for $($ty)* {
            fn slot(links: &<Self as $crate::Kind>::Links) -> &$crate::__private::Rc<$crate::__private::OneSlot<$n>> {
                &links.$i
            }
        }
        $crate::__carries!([$($g)*] [$($ty)*] [$($is)*] $($rest),*);
    };
}

/// Declares an open set of kinds: `set!(pub Holder)`. `is!` puts kinds in it, in this
/// crate or any other. A relation endpoint can be a set, and a handle can point into one.
#[macro_export]
macro_rules! set {
    ($v:vis $s:ident) => {
        #[derive(Debug)]
        $v enum $s {}
        impl $crate::Set for $s {}
        impl $crate::Handle for $s {
            type Owned = $crate::__private::Box<$crate::__private::Dyn>;
            type Ref<'t> = &'t $crate::__private::Dyn;
            fn borrow<'t>(owned: &'t $crate::__private::Box<$crate::__private::Dyn>) -> &'t $crate::__private::Dyn {
                &**owned
            }
            fn erase<'t>(r: <Self as $crate::Handle>::Ref<'t>) -> &'t $crate::__private::Dyn {
                r
            }
            fn dup(owned: &$crate::__private::Box<$crate::__private::Dyn>) -> $crate::__private::Box<$crate::__private::Dyn> {
                $crate::__private::Erased::boxed(&**owned)
            }
        }
        impl $crate::Member<$s> for $s {
            fn up<'t>(r: <Self as $crate::Handle>::Ref<'t>) -> <Self as $crate::Handle>::Ref<'t> {
                r
            }
            fn own(r: <Self as $crate::Handle>::Ref<'_>) -> $crate::__private::Box<$crate::__private::Dyn> {
                $crate::__private::Erased::boxed(r)
            }
        }
    };
}

/// Puts kinds in sets: `is!(Room, Player: Holder)`, `is!(Chest<const R: u8>: Thing, Holder)`.
#[macro_export]
macro_rules! is {
    ($($k:ident $(< const $p:ident : $pt:ty >)?),+ : $($s:ident),+) => {
        $crate::__is_each!{ [$($s),+] $($k $(< const $p : $pt >)?),+ }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __is_each {
    ([$($s:ident),+]) => {};
    ([$($s:ident),+] $k:ident < const $p:ident : $pt:ty > $(, $($rest:tt)*)?) => {
        $(
            impl<const $p: $pt> $crate::Member<$s> for $k<$p> {
                fn up<'t>(r: <Self as $crate::Handle>::Ref<'t>) -> &'t $crate::__private::Dyn {
                    r
                }
                fn own(r: <Self as $crate::Handle>::Ref<'_>) -> $crate::__private::Box<$crate::__private::Dyn> {
                    $crate::__private::Erased::boxed(r)
                }
            }
        )+
        $crate::__is_each!{ [$($s),+] $($($rest)*)? }
    };
    ([$($s:ident),+] $k:ident $(, $($rest:tt)*)?) => {
        $(
            impl $crate::Member<$s> for $k {
                fn up<'t>(r: <Self as $crate::Handle>::Ref<'t>) -> &'t $crate::__private::Dyn {
                    r
                }
                fn own(r: <Self as $crate::Handle>::Ref<'_>) -> $crate::__private::Box<$crate::__private::Dyn> {
                    $crate::__private::Erased::boxed(r)
                }
            }
        )+
        $crate::__is_each!{ [$($s),+] $($($rest)*)? }
    };
}

/// Declares a relation, with every rule it keeps, in one line.
///
/// ```text
/// relation!(pub struct In: Thing -> Holder, one, ranked, spill);
/// relation!(pub struct Exit(pub &'static str): Room <-> Room, many, cyclic, detach);
/// relation!(pub struct At: Walker -> Room, one, refuse, on_relate = arrive: Map);
/// ```
///
/// - `->` or `<->`: one-way, or two-way with a value for each direction (`many` only).
/// - `one`, `opt` or `many` targets per source.
/// - the loop rule, required if the endpoints can overlap: `ranked` or `cyclic`.
/// - what despawning a target does: `cascade`, `detach` (not for `one`), `spill` (`one`
///   only), or `refuse`.
/// - `on_relate = f: Bound`: `f` runs whenever a live turn makes an edge, and may do
///   anything `Bound` allows. Every caller that relates this relation must meet `Bound`.
#[macro_export]
macro_rules! relation {
    ($v:vis struct $name:ident $(($($f:tt)*))? : $src:ident -> $dst:ident, $($opts:tt)*) => {
        $crate::__relation_opts!{ [$v struct $name $(($($f)*))?] [one_way $src $dst] $($opts)* }
    };
    ($v:vis struct $name:ident $(($($f:tt)*))? : $src:ident <-> $dst:ident, $($opts:tt)*) => {
        $crate::__relation_opts!{ [$v struct $name $(($($f)*))?] [two_way $src $dst] $($opts)* }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __relation_opts {
    ($d:tt $e:tt $ar:ident, $lp:ident, $pol:ident, on_relate = $hook:ident : $b:path $(,)?) => {
        $crate::__relation_emit!{ $d $e $ar $lp $pol [$hook $b] }
    };
    ($d:tt $e:tt $ar:ident, $pol:ident, on_relate = $hook:ident : $b:path $(,)?) => {
        $crate::__relation_emit!{ $d $e $ar none $pol [$hook $b] }
    };
    ($d:tt $e:tt $ar:ident, $lp:ident, $pol:ident $(,)?) => {
        $crate::__relation_emit!{ $d $e $ar $lp $pol [] }
    };
    ($d:tt $e:tt $ar:ident, $pol:ident $(,)?) => {
        $crate::__relation_emit!{ $d $e $ar none $pol [] }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __relation_emit {
    ([$v:vis struct $name:ident $(($($f:tt)*))?] [$shape:ident $src:ident $dst:ident] $ar:ident $lp:ident $pol:ident [$($hook:ident $b:path)?]) => {
        #[derive(Debug)]
        $v struct $name $(($($f)*))?;
        impl $crate::__private::Part for $name {
            type Side = $crate::__private::Shared;
            type Store = $crate::__private::RelLog<Self>;
        }
        impl $crate::Relation for $name {
            type Source = $src;
            type Target = $dst;
            type Arity = $crate::__arity!($ar);
            type Loops = $crate::__loops!($lp);
            type Policy = $crate::__policy!($pol);
            type Vals = $crate::__shape!($shape vals);
            type Hook = $crate::__hook!($($hook)?);
            const NAME: &'static str = ::core::stringify!($name);
            const TWO_WAY: bool = $crate::__shape!($shape two_way);
            fn fwd(vals: &Self::Vals) -> &Self {
                $crate::__shape!($shape fwd vals)
            }
            fn back(vals: &Self::Vals) -> &Self {
                $crate::__shape!($shape back vals)
            }
            fn targets_of<'t>(log: &'t $crate::__private::RelLog<Self>, at: usize) -> $crate::__private::Vec<($crate::Id<'t, $dst>, &'t Self)> {
                $crate::__shape!($shape targets log at)
            }
            fn sources_of<'t>(log: &'t $crate::__private::RelLog<Self>, at: usize) -> $crate::__private::Vec<($crate::Id<'t, $src>, &'t Self)> {
                $crate::__shape!($shape sources log at)
            }
        }
        const _: () = if let ::core::option::Option::Some(message) = $crate::__private::check_relation::<$name>() {
            ::core::panic!("{}", message);
        };
        $crate::__shape!($shape value $name);
        $crate::__on_relate!($name $src $dst $($hook $b)?);
        $crate::__link!($ar $name $src $dst);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __arity {
    (one) => { $crate::One };
    (opt) => { $crate::Opt };
    (many) => { $crate::Many };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __loops {
    (none) => { $crate::NoLoops };
    (ranked) => { $crate::Ranked };
    (cyclic) => { $crate::Cyclic };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __policy {
    (cascade) => { $crate::Cascade };
    (detach) => { $crate::Detach };
    (spill) => { $crate::Spill };
    (refuse) => { $crate::Refuse };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __hook {
    () => { $crate::Sure };
    ($hook:ident) => { $crate::Refused };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __shape {
    (one_way vals) => { Self };
    (two_way vals) => { (Self, Self) };
    (one_way two_way) => { false };
    (two_way two_way) => { true };
    (one_way fwd $v:ident) => { $v };
    (one_way back $v:ident) => { $v };
    (two_way fwd $v:ident) => { &$v.0 };
    (two_way back $v:ident) => { &$v.1 };
    (one_way targets $log:ident $at:ident) => { $crate::__private::one_way_targets($log, $at) };
    (one_way sources $log:ident $at:ident) => { $crate::__private::one_way_sources($log, $at) };
    (two_way targets $log:ident $at:ident) => { $crate::__private::two_way($log, $at) };
    (two_way sources $log:ident $at:ident) => { $crate::__private::two_way($log, $at) };
    (one_way value $name:ident) => {
        impl $crate::Value for $name {
            type Relation = $name;
            fn vals(self) -> Self {
                self
            }
        }
    };
    (two_way value $name:ident) => {
        impl $crate::TwoWay for $name {}
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __on_relate {
    ($name:ident $src:ident $dst:ident) => {
        impl<G: $crate::Schema> $crate::OnRelate<G> for $name {
            fn fire<'t>(
                _: &mut $crate::Turn<'t, G, $crate::Live>,
                _: $crate::Id<'t, $src>,
                _: $crate::Id<'t, $dst>,
            ) -> ::core::result::Result<(), ::core::convert::Infallible> {
                ::core::result::Result::Ok(())
            }
        }
    };
    ($name:ident $src:ident $dst:ident $hook:ident $b:path) => {
        impl<G: $b> $crate::OnRelate<G> for $name {
            fn fire<'t>(
                t: &mut $crate::Turn<'t, G, $crate::Live>,
                source: $crate::Id<'t, $src>,
                target: $crate::Id<'t, $dst>,
            ) -> ::core::result::Result<(), $crate::Refused> {
                $hook(t, source, target)
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __link {
    (one $name:ident $src:ident $dst:ident) => {
        impl<X: $crate::__private::Carries<$name> + $crate::Member<$src>, Y: $crate::Member<$dst>> $crate::__private::Link<X, Y> for $name {
            fn link<'t>(log: &'t $crate::__private::RelLog<Self>, s: $crate::Id<'t, X>, value: Self, d: $crate::Id<'t, Y>) {
                $crate::__private::link_one(log, s, value, d)
            }
        }
        impl<'t, K: $crate::__private::Carries<$name>> $crate::Holds<'t, $name> for $crate::Id<'t, K> {
            fn read(self, _: &'t $crate::__private::RelLog<$name>) -> $crate::Id<'t, $dst> {
                $crate::__private::read_one::<$name, K>(self)
            }
        }
    };
    (opt $name:ident $src:ident $dst:ident) => {
        impl<X: $crate::Member<$src>, Y: $crate::Member<$dst>> $crate::__private::Link<X, Y> for $name {
            fn link<'t>(
                log: &'t $crate::__private::RelLog<Self>,
                s: $crate::Id<'t, X>,
                vals: <Self as $crate::Relation>::Vals,
                d: $crate::Id<'t, Y>,
            ) {
                $crate::__private::link_opt(log, s, vals, d)
            }
        }
        impl<'t, K: $crate::Kind + $crate::Member<$src>> $crate::Holds<'t, $name> for $crate::Id<'t, K> {
            fn read(self, log: &'t $crate::__private::RelLog<$name>) -> ::core::option::Option<$crate::Id<'t, $dst>> {
                $crate::__private::read_opt::<$name, K>(self, log)
            }
        }
        impl $crate::Detachable for $name {}
    };
    (many $name:ident $src:ident $dst:ident) => {
        impl<X: $crate::Member<$src>, Y: $crate::Member<$dst>> $crate::__private::Link<X, Y> for $name {
            fn link<'t>(
                log: &'t $crate::__private::RelLog<Self>,
                s: $crate::Id<'t, X>,
                vals: <Self as $crate::Relation>::Vals,
                d: $crate::Id<'t, Y>,
            ) {
                $crate::__private::link_many(log, s, vals, d)
            }
        }
        impl $crate::Detachable for $name {}
    };
}

/// Declares a component: changeable state for entities of one kind.
/// `component!(pub struct Lit(pub bool): Item)`.
#[macro_export]
macro_rules! component {
    ($v:vis struct $name:ident $(($($f:tt)*))? : $k:ty) => {
        #[derive(Debug)]
        $v struct $name $(($($f)*))?;
        impl $crate::__private::Part for $name {
            type Side = $crate::__private::Owned;
            type Store = $crate::__private::Table<Self>;
        }
        impl $crate::Component for $name {
            type Kind = $k;
        }
    };
}

/// Declares a module: a named group of kinds, relations and components that games include.
///
/// ```text
/// module!(pub Map { kinds: [Room, Player], relations: [Exit, At], components: [Lit] });
/// ```
///
/// It defines a trait, `Map`, that every game including the module meets: a bound of
/// `G: Map` gives generic code all of the module's parts. It also defines a macro of the
/// same name, which `game!` calls by path, so a module from another crate drops into a game
/// as `game!(Game: Settings [other_crate::Map])`. Crates that declare modules enable
/// `#![feature(decl_macro)]`.
#[macro_export]
macro_rules! module {
    ($v:vis $name:ident {
        kinds: [$($k:ty),* $(,)?],
        relations: [$($r:ty),* $(,)?]
        $(, components: [$($c:ty),* $(,)?])?
        $(,)?
    }) => {
        $v trait $name: $crate::Schema $(+ $crate::Has<$k>)* $(+ $crate::Has<$r>)* $($(+ $crate::Has<$c>)*)? {}
        impl<G: $crate::Schema $(+ $crate::Has<$k>)* $(+ $crate::Has<$r>)* $($(+ $crate::Has<$c>)*)?> $name for G {}
        $crate::__module_macro!{ ($) $v $name [$($k,)*] [$($r,)*] [$($($c,)*)?] }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __module_macro {
    (($d:tt) $v:vis $name:ident [$($k:ty,)*] [$($r:ty,)*] [$($c:ty,)*]) => {
        $v macro $name(@collect $d h:tt $d rest:tt [$d($d ak:ty,)*] [$d($d ar:ty,)*] [$d($d ac:ty,)*]) {
            $crate::__game_step!{ @next $d h $d rest [$d($d ak,)* $($k,)*] [$d($d ar,)* $($r,)*] [$d($d ac,)* $($c,)*] }
        }
    };
}

/// Declares a game: a root kind and the modules it includes.
/// `game!(pub Game: Settings [Map, containers::Containers])`.
///
/// Under `cargo check`, it also rejects declarations that could break the graph:
/// - a kind that can be the source of a `one` relation but doesn't `need` it;
/// - a relation with no loop rule whose endpoints overlap;
/// - a `cascade` or `spill` relation the root can be the source of;
/// - `one` relations that could make an entity own itself, directly or through others.
#[macro_export]
macro_rules! game {
    ($v:vis $name:ident : $root:ty [$($m:tt)*]) => {
        $crate::__game_step!{ @next [$v $name : $root] [$($m)*] [] [] [] }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __game_step {
    (@next $h:tt [] $k:tt $r:tt $c:tt) => {
        $crate::__game_emit!{ $h $k $r $c }
    };
    (@next $h:tt [$($seg:ident)::+ $(, $($rest:tt)*)?] $k:tt $r:tt $c:tt) => {
        $($seg)::+ ! { @collect $h [$($($rest)*)?] $k $r $c }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __game_emit {
    ([$v:vis $name:ident : $root:ty] [$($k:ty,)*] [$($r:ty,)*] [$($c:ty,)*]) => {
        $v struct $name;
        impl $crate::Schema for $name {
            type Root = $root;
            type Shared = $crate::__private::hlist!(
                $crate::__private::KindList<$root>,
                $(<$k as $crate::__private::Part>::Store,)*
                $(<$r as $crate::__private::Part>::Store,)*
            );
            type Comps = $crate::__private::hlist!($(<$c as $crate::__private::Part>::Store,)*);
            fn root_list(shared: &Self::Shared) -> &$crate::__private::KindList<$root> {
                &shared.0
            }
        }
        $crate::__has!{ $name [.1] [$($k,)* $($r,)*] }
        $crate::__has!{ $name [] [$($c,)*] }
        $crate::__probe!{ [$root, $($k,)*] [$($r,)*] }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __has {
    ($g:ident [$($p:tt)*] []) => {};
    ($g:ident [$($p:tt)*] [$x:ty, $($rest:ty,)*]) => {
        impl $crate::Has<$x> for $g {
            fn part(
                s: &<<$x as $crate::__private::Part>::Side as $crate::__private::Side>::Of<Self>,
            ) -> &<$x as $crate::__private::Part>::Store {
                &s $($p)* .0
            }
            fn part_mut(
                s: &mut <<$x as $crate::__private::Part>::Side as $crate::__private::Side>::Of<Self>,
            ) -> &mut <$x as $crate::__private::Part>::Store {
                &mut s $($p)* .0
            }
        }
        $crate::__has!{ $g [$($p)* .1] [$($rest,)*] }
    };
}

/// The declaration checks, as consts that fail `cargo check` with a message.
#[doc(hidden)]
#[macro_export]
macro_rules! __probe {
    ([$root:ty, $($k:ty,)*] $rels:tt) => {
        $crate::__probe_with!{ [$root, $($k,)*] [$root, $($k,)*] $rels }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __probe_with {
    ([$root:ty, $($k:ty,)*] $all:tt $rels:tt) => {
        const _: () = {
            #[allow(unused_imports)]
            use $crate::__private::Otherwise as _;
            $crate::__probe_kind!{ $root; $rels }
            $($crate::__probe_kind!{ $k; $rels })*
            $crate::__probe_root!{ $root; $rels }
            const OWNERS: &[&str] = &[$crate::__owns_message!($root), $($crate::__owns_message!($k),)*];
            const OWNS: &[&[bool]] = &[
                $crate::__owns_row!{ $root; $all; $rels },
                $($crate::__owns_row!{ $k; $all; $rels },)*
            ];
            if let ::core::option::Option::Some(i) = $crate::__private::owns_itself(OWNS) {
                ::core::panic!("{}", $crate::__private::nth(OWNERS, i));
            }
        };
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __probe_root {
    ($root:ty; [$($r:ty,)*]) => {
        $(
            if <$crate::__private::Probe<$root, $r>>::TAKES {
                ::core::panic!(::core::concat!(
                    "the root `", ::core::stringify!($root), "` can be the source of `", ::core::stringify!($r),
                    "`, which may not `cascade` or `spill`: the root is never despawned or moved",
                ));
            }
        )*
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __probe_kind {
    ($k:ty; [$($r:ty,)*]) => {
        $(
            if <$crate::__private::Probe<$k, $r>>::ONE_SOURCE && !<$crate::__private::Probe<$k, $r>>::NEEDED {
                ::core::panic!(::core::concat!(
                    "`", ::core::stringify!($k), "` can be the source of the `one` relation `", ::core::stringify!($r),
                    "`, so it must `need` it: `kind!(", ::core::stringify!($k), " needs ", ::core::stringify!($r), ")`",
                ));
            }
            if <$crate::__private::Probe<$k, $r>>::LOOPLESS_OVERLAP {
                ::core::panic!(::core::concat!(
                    "`", ::core::stringify!($k), "` can be both the source and the target of `", ::core::stringify!($r),
                    "`, so `", ::core::stringify!($r), "` needs a loop rule: `ranked` or `cyclic`",
                ));
            }
        )*
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __owns_row {
    ($a:ty; [$($b:ty,)*]; $rels:tt) => {
        &[$($crate::__owns_cell!{ $a; $b; $rels }),*]
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __owns_cell {
    ($a:ty; $b:ty; [$($r:ty,)*]) => {
        false $(|| <$crate::__private::Probe<$a, $r, $b>>::OWNS)*
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __owns_message {
    ($k:ty) => {
        ::core::concat!(
            "through `one` edges, a `", ::core::stringify!($k),
            "` could own itself: make one of the relations on that loop `opt` or `many`, or `ranked` with rising ranks",
        )
    };
}
