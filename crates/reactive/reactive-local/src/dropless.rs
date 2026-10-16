/// Declares thread-locals as [`std::thread_local!`] does, and refuses to compile one whose type has a destructor.
///
/// On glibc a thread-local whose type has a destructor pins the library that registered it until its thread exits, so one a hot-reloaded or embedded app reaches keeps that whole build mapped, code and statics, after its host has unloaded it. Every crate an app links declares its thread-locals with this macro instead, which the `every_thread_local_an_app_links_is_dropless` test holds the workspace to.
///
/// A slot holding the app's state is declared `released`: it is a [`Released`](crate::thread_exit::Released), a `RefCell` that hands its own release to [`on_release`](crate::thread_exit::on_release) the first time it is reached, so what it holds is freed when its thread is released, as a host releases a guest's before unloading it. Anything else that needs freeing goes in a [`ManuallyDrop`](std::mem::ManuallyDrop) or behind a raw pointer, its release handed to [`thread_exit`](crate::thread_exit) by hand.
///
/// ```
/// use std::cell::Cell;
///
/// telar_reactive_local::dropless_thread_local! {
///     static DEPTH: Cell<usize> = const { Cell::new(0) };
///     static NAMES: released Vec<String> = const { Vec::new() };
/// }
/// # DEPTH.with(|depth| depth.set(1));
/// # NAMES.with(|names| names.borrow_mut().push("kept".to_owned()));
/// ```
///
/// ```compile_fail,E0080
/// telar_reactive_local::dropless_thread_local! {
///     static NAMES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
/// }
/// ```
#[macro_export]
macro_rules! dropless_thread_local {
    () => {};
    ($(#[$attr:meta])* $vis:vis static $name:ident: released $t:ty = const $init:block $(; $($rest:tt)*)?) => {
        ::std::thread_local!($(#[$attr])* $vis static $name: $crate::thread_exit::Released<$t> = const {
            $crate::thread_exit::Released::new($init, || $init, || $name.with($crate::thread_exit::Released::release))
        });
        $($crate::dropless_thread_local!($($rest)*);)?
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: released $t:ty = $init:expr $(; $($rest:tt)*)?) => {
        ::std::thread_local!($(#[$attr])* $vis static $name: $crate::thread_exit::Released<$t> =
            $crate::thread_exit::Released::new($init, || $init, || $name.with($crate::thread_exit::Released::release))
        );
        $($crate::dropless_thread_local!($($rest)*);)?
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = const $init:block $(; $($rest:tt)*)?) => {
        ::std::thread_local!($(#[$attr])* $vis static $name: $t = const {
            $crate::__refuse_destructor!($name: $t);
            $init
        });
        $($crate::dropless_thread_local!($($rest)*);)?
    };
    ($(#[$attr:meta])* $vis:vis static $name:ident: $t:ty = $init:expr $(; $($rest:tt)*)?) => {
        ::std::thread_local!($(#[$attr])* $vis static $name: $t = {
            $crate::__refuse_destructor!($name: $t);
            $init
        });
        $($crate::dropless_thread_local!($($rest)*);)?
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __refuse_destructor {
    ($name:ident: $t:ty) => {
        const _: () = ::std::assert!(
            !::std::mem::needs_drop::<$t>(),
            ::std::concat!(
                "thread-local `",
                ::std::stringify!($name),
                "` has a destructor, which would keep a hot-reloaded or embedded app's library mapped after it is unloaded: hold its value in a `ManuallyDrop` and free it through `thread_exit`",
            ),
        );
    };
}

#[cfg(test)]
#[path = "dropless_test.rs"]
mod tests;
