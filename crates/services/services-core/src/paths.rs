use std::path::PathBuf;

pub trait AppPathsProvider: Send + Sync {
    fn config_dir(&self) -> Option<PathBuf>;
    fn data_dir(&self) -> Option<PathBuf>;
    fn cache_dir(&self) -> Option<PathBuf>;
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_mock_provider_config_dir() {
        let provider = MockPathsProvider {
            config: PathBuf::from("/mock/config"),
            data: PathBuf::from("/mock/data"),
            cache: PathBuf::from("/mock/cache"),
        };

        assert_eq!(provider.config_dir(), Some(PathBuf::from("/mock/config")));
    }

    #[test]
    fn test_mock_provider_data_dir() {
        let provider = MockPathsProvider {
            config: PathBuf::from("/mock/config"),
            data: PathBuf::from("/mock/data"),
            cache: PathBuf::from("/mock/cache"),
        };

        assert_eq!(provider.data_dir(), Some(PathBuf::from("/mock/data")));
    }

    #[test]
    fn test_mock_provider_cache_dir() {
        let provider = MockPathsProvider {
            config: PathBuf::from("/mock/config"),
            data: PathBuf::from("/mock/data"),
            cache: PathBuf::from("/mock/cache"),
        };

        assert_eq!(provider.cache_dir(), Some(PathBuf::from("/mock/cache")));
    }

    struct NonePathsProvider;

    impl AppPathsProvider for NonePathsProvider {
        fn config_dir(&self) -> Option<PathBuf> {
            None
        }

        fn data_dir(&self) -> Option<PathBuf> {
            None
        }

        fn cache_dir(&self) -> Option<PathBuf> {
            None
        }
    }

    #[test]
    fn test_none_provider_handles_missing_paths() {
        let provider = NonePathsProvider;

        assert_eq!(provider.config_dir(), None);
        assert_eq!(provider.data_dir(), None);
        assert_eq!(provider.cache_dir(), None);
    }
}
