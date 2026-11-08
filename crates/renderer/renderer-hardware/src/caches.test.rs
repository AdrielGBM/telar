// A global behind a Mutex needs `Send`.
#[test]
fn shared_caches_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<super::SharedCaches>();
}
