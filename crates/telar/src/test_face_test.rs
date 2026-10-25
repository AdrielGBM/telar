ctor::declarative::ctor! {
    #[ctor(unsafe, anonymous)]
    fn set_every_unit_test_in_the_test_face() {
        crate::testing::install_test_face();
    }
}
