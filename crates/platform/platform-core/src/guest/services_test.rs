use super::*;

struct Greeter {
    greeting: String,
}

unsafe extern "C" fn greet(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |greeter: &Greeter, name: String| {
            format!("{}, {name}", greeter.greeting)
        })
    }
}

unsafe extern "C" fn panic_while_serving(data: *const c_void, request: LentBytes, reply: Reply) {
    unsafe {
        serve(data, request, reply, |_: &Greeter, (): ()| -> String {
            panic!("the backend went away")
        })
    }
}

fn table(greeter: &Greeter) -> HostServices {
    HostServices {
        data: std::ptr::from_ref(greeter).cast(),
        clipboard_text: Some(greet),
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
    let answer: Option<String> = unsafe { table.ask(table.clipboard_text, &"guest".to_owned()) };
    assert_eq!(answer.as_deref(), Some("hello, guest"));
}

#[test]
fn a_missing_entry_a_panicking_one_and_a_request_it_cannot_read_all_answer_nothing() {
    let greeter = Greeter {
        greeting: "hello".to_owned(),
    };
    let table = table(&greeter);
    assert_eq!(unsafe { table.ask::<String>(table.open_uri, &()) }, None);
    assert_eq!(
        unsafe { table.ask::<String>(table.set_clipboard_text, &()) },
        None
    );
    assert_eq!(
        unsafe { table.ask::<String>(table.clipboard_text, &7u8) },
        None
    );
}

#[test]
fn an_answer_that_is_no_value_of_the_type_asked_for_is_none() {
    let greeter = Greeter {
        greeting: "hello".to_owned(),
    };
    let table = table(&greeter);
    assert_eq!(
        unsafe { table.ask::<u64>(table.clipboard_text, &"guest".to_owned()) },
        None
    );
}

#[test]
fn a_reply_nobody_sends_to_collects_nothing() {
    assert_eq!(Reply::collect(drop), None);
    let sent = Reply::collect(|reply| unsafe { reply.send(b"bytes") });
    assert_eq!(sent.as_deref(), Some(&b"bytes"[..]));
}

#[test]
fn every_field_of_the_table_is_one_pointer_wide_and_hashed() {
    let layout = HOST_SERVICES_LAYOUT;
    assert_eq!(layout.name, "HostServices");
    assert_eq!(
        layout.size,
        std::mem::size_of::<*const c_void>() * layout.fields.len()
    );
}
