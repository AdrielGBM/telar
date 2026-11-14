use super::*;

#[test]
fn a_provider_with_no_paths_answers_none_to_every_directory() {
    let provider = NoPaths;

    assert_eq!(provider.config_dir(), None);
    assert_eq!(provider.data_dir(), None);
    assert_eq!(provider.cache_dir(), None);
}
