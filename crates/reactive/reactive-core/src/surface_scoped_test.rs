use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Size {
    Small,
    Large,
}

crate::surface_scoped! {
    scoped size: Size = Size::Small;
    context SizeContext, SizeGuard;
}

crate::surface_scoped! {
    scoped name: String as Option<String> = None;
    context NameContext, NameGuard;
}

fn record<T: 'static>(read: impl Fn() -> T + 'static) -> (Rc<RefCell<Vec<T>>>, crate::Effect) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let effect = crate::effect(move || sink.borrow_mut().push(read()));
    (seen, effect)
}

#[test]
fn setting_the_value_already_there_reruns_nobody() {
    size().set(Size::Small);
    let (seen, _effect) = record(|| size().get());
    size().set(Size::Large);
    size().set(Size::Large);
    assert_eq!(*seen.borrow(), vec![Size::Small, Size::Large]);

    let surface = SizeContext::new();
    let (surface_seen, _surface_effect) = {
        let _entered = surface.enter();
        record(|| size().surface())
    };
    {
        let _entered = surface.enter();
        size().set_surface(Some(&Size::Small));
        size().set_surface(Some(&Size::Small));
    }
    assert_eq!(*surface_seen.borrow(), vec![None, Some(Size::Small)]);
    size().set(Size::Small);
}

#[test]
fn an_override_shadows_the_thread_value_on_its_surface_alone() {
    size().set(Size::Small);
    let surface = SizeContext::new();
    let (seen, _effect) = {
        let _entered = surface.enter();
        record(|| size().get())
    };
    {
        let _entered = surface.enter();
        size().set_surface(Some(&Size::Large));
        assert_eq!(size().peek(), Size::Large);
        assert_eq!(
            size().peek_thread(),
            Size::Small,
            "the thread's value is still there under the override"
        );
    }
    assert_eq!(
        size().peek(),
        Size::Small,
        "the ambient world keeps the thread's"
    );

    size().set(Size::Large);
    size().set(Size::Small);
    assert_eq!(
        *seen.borrow(),
        vec![Size::Small, Size::Large],
        "an overridden reader does not hear the thread's value move"
    );

    {
        let _entered = surface.enter();
        size().set_surface(None::<&Size>);
        assert_eq!(size().peek(), Size::Small);
    }
    size().set(Size::Large);
    assert_eq!(
        *seen.borrow(),
        vec![Size::Small, Size::Large, Size::Small, Size::Large],
        "once cleared, the surface follows the thread again"
    );
    size().set(Size::Small);
}

#[test]
fn an_owned_value_overrides_borrowed_and_resolves_into_the_threads_type() {
    name().set(None);
    let surface = NameContext::new();
    let (seen, _effect) = {
        let _entered = surface.enter();
        record(|| name().get())
    };
    {
        let _entered = surface.enter();
        name().set_surface(Some("surface"));
        name().set_surface(Some("surface"));
        assert_eq!(name().surface().as_deref(), Some("surface"));
    }
    name().set(Some("thread".to_owned()));
    assert_eq!(name().peek().as_deref(), Some("thread"));
    {
        let _entered = surface.enter();
        name().set_surface(None::<&str>);
    }
    assert_eq!(
        *seen.borrow(),
        vec![None, Some("surface".to_owned()), Some("thread".to_owned())]
    );
    name().set(None);
}
