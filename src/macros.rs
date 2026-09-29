//! `set!` and `is!`. The other declaration macros are in `weft-macros`.

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

/// Puts kinds in sets: `is!(Room, Player: Holder)`, `is!(Chest<R>: Thing, Holder)`.
#[macro_export]
macro_rules! is {
    ($($k:ident $(< $p:ident >)?),+ : $($s:ident),+) => {
        $crate::__is_each!{ [$($s),+] $($k $(< $p >)?),+ }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __is_each {
    ([$($s:ident),+]) => {};
    ([$($s:ident),+] $k:ident < $p:ident > $(, $($rest:tt)*)?) => {
        $(
            impl<$p: $crate::Rank> $crate::Member<$s> for $k<$p> {
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
