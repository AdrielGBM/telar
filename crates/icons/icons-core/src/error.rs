use std::path::PathBuf;

/// Why an icon could not be resolved. An icon a source simply does not have is not an error: every lookup answers `Ok(None)` for that, so a chain of sources can fall through to the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconError {
    /// An id that is not `set:name` in Iconify's alphabet.
    InvalidId { id: String, reason: &'static str },
    /// A bare name, written without its set where no default set is configured to read it in.
    MissingSet { name: String },
    /// A file a source reads that exists and could not be read or parsed.
    Read { path: PathBuf, message: String },
    /// A provider that failed to answer, or answered with something that is not an Iconify set.
    Fetch { url: String, message: String },
}

impl std::fmt::Display for IconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId { id, reason } => write!(f, "`{id}` is not an icon id: {reason}"),
            Self::MissingSet { name } => write!(
                f,
                "`{name}` names no icon set and no default set is configured: write it as `set:name`, like `mdi:{name}`"
            ),
            Self::Read { path, message } => write!(f, "{}: {message}", path.display()),
            Self::Fetch { url, message } => write!(f, "{url}: {message}"),
        }
    }
}

impl std::error::Error for IconError {}
