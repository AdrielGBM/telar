use super::*;

/// A context is written by every build, not provided by the first one — or a rebuilt panel would still be showing the page, the radius and the config the build before it was given.
#[test]
fn a_second_build_replaces_the_context_the_first_one_set() {
    #[derive(Clone, PartialEq, Debug)]
    struct Ctx(&'static str);

    Scope::with(|| {
        set_context(Ctx("first"));
        set_context(Ctx("second"));
        assert_eq!(context::<Ctx>(), Some(Ctx("second")));
    });
}

/// A surface builder writes its context with the owner stack empty, so it lands on the surface's root. A component built under it opens a scope of its own — and unless that scope is parented by the same rule, it is an orphan and the walk up never reaches what the builder said. A drawer asking which module it shows read an empty string this way, and drew the fallback panel for every module.
#[test]
fn a_scope_opened_with_an_empty_stack_still_sees_the_surface_context() {
    #[derive(Clone, PartialEq, Debug)]
    struct Ctx(&'static str);

    reactive_core::reset_runtime();
    set_context(Ctx("battery"));
    let _scope = reactive_core::owner_scope();
    assert_eq!(context::<Ctx>(), Some(Ctx("battery")));
}

/// Two components saying different things about their own subtrees must not collide, which is what a scope *per component* buys and what nothing else does. Without one they share whoever built them: the second `provide` is refused as a repeat, and — worse than the error — it reads the first one's value.
///
/// The transpiler opens the scope; this pins the behaviour that depends on it.
#[test]
fn two_sibling_scopes_each_provide_their_own() {
    #[derive(Clone, PartialEq, Debug)]
    struct Ctx(u8);

    let seen = |value: u8| {
        Scope::with(|| {
            provide(Ctx(value)).expect("a fresh scope has provided nothing");
            context::<Ctx>()
        })
    };

    assert_eq!(seen(1), Some(Ctx(1)));
    assert_eq!(
        seen(2),
        Some(Ctx(2)),
        "and the second is not shown the first"
    );
}

/// Nothing set is `None` rather than a default, so a widget built outside a surface says so.
#[test]
fn an_unset_context_is_absent() {
    #[derive(Clone, PartialEq, Debug)]
    struct Unset(u8);

    Scope::with(|| assert_eq!(context::<Unset>(), None));
}

/// The reason this phase exists. A handler runs long after the build that made it returned, and a call-stack scope has closed by then — so the value a component provided for its own subtree was readable everywhere except from the events that subtree raises.
///
/// **Backing the registry with the tree is only half of it.** A handler is a plain closure: when it runs there is no owner stack, so an ambient read resolves against the surface root rather than against the component. Something has to put it back in its owner. Here that is explicit; in a real tree it is `dispatch_container_event`, which re-enters the child's owner around `on_event` the same way the reactive flush re-enters an effect's surface.
#[test]
fn a_context_provided_during_a_build_is_readable_from_a_handler_that_fires_later() {
    #[derive(Clone, PartialEq, Debug)]
    struct Desk(u8);

    let scope = reactive_core::owner_scope();
    let owner = scope.id();
    set_context(Desk(7));
    let handler: Box<dyn Fn() -> Option<Desk>> = Box::new(context::<Desk>);
    drop(scope);

    assert_eq!(
        reactive_core::with_owner(Some(owner), &handler),
        Some(Desk(7)),
        "the build is long over"
    );

    reactive_core::dispose_owner(owner);
    assert_eq!(
        reactive_core::with_owner(Some(owner), &handler),
        None,
        "and disposing the owner takes it away"
    );
}
