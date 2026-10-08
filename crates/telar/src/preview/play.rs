//! Play functions: scripted interactions that run against a mounted preview.

use std::fmt;

/// The canvas a play function drives.
#[non_exhaustive]
#[derive(Debug)]
pub struct Play {}

/// Why a play function stopped.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayError {
    pub message: String,
}

impl PlayError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PlayError {}

pub type PlayResult = Result<(), PlayError>;

pub type PlayFn = fn(&mut Play) -> PlayResult;
