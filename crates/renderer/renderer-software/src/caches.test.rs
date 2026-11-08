use super::*;

/// What a host does to a hot-reloaded copy's thread before unloading it, and what any thread does as it exits: the caches are freed, and the next frame builds them again.
#[test]
fn a_released_thread_frees_its_caches_and_builds_them_again_when_asked() {
    with_caches(|_| ());
    assert!(initialised(), "precondition: the thread has caches");
    reactive_local::thread_exit::release_thread();
    assert!(!initialised(), "the release frees them");
    with_caches(|_| ());
    assert!(initialised(), "and the next use builds them again");
}
