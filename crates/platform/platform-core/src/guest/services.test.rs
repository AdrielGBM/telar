use super::*;
use crate::guest::Lifecycle;

struct Greeter {
    greeting: String,
}

unsafe extern "C" fn greet(
    data: *const c_void,
    request: LentBytes<String>,
    reply: Reply<Option<String>>,
) {
    unsafe {
        serve(data, request, reply, |greeter: &Greeter, name: String| {
            Some(format!("{}, {name}", greeter.greeting))
        })
    }
}

unsafe extern "C" fn panic_while_serving(
    data: *const c_void,
    request: LentBytes<String>,
    reply: Reply<()>,
) {
    unsafe {
        serve(data, request, reply, |_: &Greeter, _: String| {
            panic!("the backend went away")
        })
    }
}

unsafe extern "C" fn answer_with_a_word(
    data: *const c_void,
    request: LentBytes<String>,
    reply: Reply<Option<String>>,
) {
    unsafe {
        serve(data, request, reply, |_: &Greeter, _: String| {
            Some("not a stored value".to_owned())
        })
    }
}

fn table(greeter: &Greeter) -> HostServices<NoUpper> {
    HostServices {
        data: std::ptr::from_ref(greeter).cast(),
        stored_preference: Some(greet),
        set_clipboard_text: Some(panic_while_serving),
        ..HostServices::NONE
    }
}

#[test]
fn an_entry_answers_with_bytes_the_asking_side_reads_back_as_its_own() {
    let greeter = Greeter {
        greeting: "hello".to_owned(),
    };
    let table = table(&greeter);
    let answer = unsafe { table.ask(table.stored_preference, &"guest".to_owned()) };
    assert_eq!(answer, Some(Some("hello, guest".to_owned())));
}

#[test]
fn a_missing_entry_and_a_panicking_one_answer_nothing() {
    let greeter = Greeter {
        greeting: "hello".to_owned(),
    };
    let table = table(&greeter);
    assert_eq!(unsafe { table.ask(table.clipboard_text, &()) }, None);
    assert_eq!(
        unsafe { table.ask(table.set_clipboard_text, &"text".to_owned()) },
        None
    );
}

#[test]
fn a_reply_nobody_sends_to_collects_nothing() {
    assert_eq!(Reply::<String>::collect(drop), None);
    let sent = Reply::<String>::collect(|reply| unsafe { reply.send(&"answer".to_owned()) });
    assert_eq!(sent, Some(encode(&"answer".to_owned())));
}

#[test]
fn a_host_and_a_guest_agree_on_what_an_entry_carries_or_do_not_compile() {
    let greeter = Greeter {
        greeting: "hello".to_owned(),
    };
    let table = HostServices::<NoUpper> {
        data: std::ptr::from_ref(&greeter).cast(),
        stored_preference: Some(answer_with_a_word),
        ..HostServices::NONE
    };
    let answer: Option<Option<String>> =
        unsafe { table.ask(table.stored_preference, &"key".to_owned()) };
    assert_eq!(answer, Some(Some("not a stored value".to_owned())));
}

#[test]
fn every_field_of_the_table_but_the_upper_one_is_one_pointer_wide_and_hashed() {
    let layout = HostServices::<NoUpper>::LAYOUT;
    assert_eq!(layout.name, "HostServices<Upper>");
    assert_eq!(
        layout.size,
        std::mem::size_of::<*const c_void>() * (layout.fields.len() - 1)
            + std::mem::size_of::<NoUpper>()
    );
}

/// An `upper` table named `Upper` in a module of its own, so two of them differ in nothing but the answer of their one service: `$answer`.
macro_rules! upper_table {
    ($module:ident, $answer:ty) => {
        mod $module {
            use super::*;

            #[derive(Clone, Copy, Debug)]
            #[repr(C)]
            pub(super) struct Upper {
                dialog: Service<String, $answer>,
            }

            impl ArgShape for Upper {
                const ARG_SHAPE: u64 = <Self as UpperServices>::LAYOUT.shape;
            }

            impl UpperServices for Upper {
                const NONE: Self = Self { dialog: None };
                const LAYOUT: TypeLayout = crate::type_layout!(Upper { dialog });
            }
        }
    };
}

upper_table!(path, Option<String>);
upper_table!(count, Option<u32>);

#[test]
fn what_a_service_answers_is_in_the_shape_of_every_table_that_carries_it() {
    assert_ne!(
        path::Upper::LAYOUT.shape,
        count::Upper::LAYOUT.shape,
        "named alike, so only the answer tells them apart"
    );
    let path = HostServices::<path::Upper>::LAYOUT.shape;
    let count = HostServices::<count::Upper>::LAYOUT.shape;
    assert_ne!(path, count, "the host's table carries it");
    assert_ne!(
        Lifecycle::<path::Upper>::LAYOUT.shape,
        Lifecycle::<count::Upper>::LAYOUT.shape,
        "and so does the entry that installs it"
    );
}

#[test]
fn what_a_service_is_asked_is_in_its_shape() {
    type AskedName = Service<String, ()>;
    type AskedCount = Service<u32, ()>;
    assert_ne!(AskedName::ARG_SHAPE, AskedCount::ARG_SHAPE);
}
