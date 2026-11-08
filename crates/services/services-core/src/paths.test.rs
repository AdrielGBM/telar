use super::*;

struct MockPathsProvider {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

impl AppPathsProvider for MockPathsProvider {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.config.clone())
    }

    fn data_dir(&self) -> Option<PathBuf> {
        Some(self.data.clone())
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.cache.clone())
    }
}

fn mock() -> MockPathsProvider {
    MockPathsProvider {
        config: PathBuf::from("/mock/config"),
        data: PathBuf::from("/mock/data"),
        cache: PathBuf::from("/mock/cache"),
    }
}

#[test]
fn a_provider_hands_back_the_config_dir_it_was_given() {
    assert_eq!(mock().config_dir(), Some(PathBuf::from("/mock/config")));
}

#[test]
fn a_provider_hands_back_the_data_dir_it_was_given() {
    assert_eq!(mock().data_dir(), Some(PathBuf::from("/mock/data")));
}

#[test]
fn a_provider_hands_back_the_cache_dir_it_was_given() {
    assert_eq!(mock().cache_dir(), Some(PathBuf::from("/mock/cache")));
}

/// The three directories are separate questions, so a provider with nothing to offer has to answer `None` to each rather than to the first one asked.
#[test]
fn a_provider_with_no_paths_answers_none_to_every_directory() {
    let provider = NoPaths;

    assert_eq!(provider.config_dir(), None);
    assert_eq!(provider.data_dir(), None);
    assert_eq!(provider.cache_dir(), None);
}
