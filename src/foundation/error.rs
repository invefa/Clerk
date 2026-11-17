//! Errors and process exit codes.
//!
//! The exit code is part of the command contract: hooks and scripts branch on
//! it, so the mapping lives in one place.

use std::fmt;

pub const EXIT_OK: i32 = 0;
pub const EXIT_CHECK: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_VALIDATION: i32 = 3;
pub const EXIT_CONFLICT: i32 = 4;
pub const EXIT_STALE: i32 = 5;

/// Why an operation stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The command line, the workspace or a declaration is unusable.
    Usage,
    /// Reading or writing a file failed.
    Io,
    /// A value breaks a declared rule.
    Validation,
    /// The instance is not in a state that allows the operation.
    Conflict,
    /// The data was changed since the operation started.
    Stale,
    /// A check found problems in otherwise readable data.
    Check,
}

impl Failure {
    pub fn code(self) -> i32 {
        match self {
            Failure::Check => EXIT_CHECK,
            Failure::Usage => EXIT_USAGE,
            Failure::Validation => EXIT_VALIDATION,
            Failure::Conflict => EXIT_CONFLICT,
            Failure::Stale => EXIT_STALE,
            Failure::Io => EXIT_CHECK,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Error {
    failure: Failure,
    message: String,
}

impl Error {
    pub fn new(failure: Failure, message: impl Into<String>) -> Self {
        Error {
            failure,
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Error::new(Failure::Usage, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Error::new(Failure::Io, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Error::new(Failure::Validation, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Error::new(Failure::Conflict, message)
    }

    pub fn failure(&self) -> Failure {
        self.failure
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn code(&self) -> i32 {
        self.failure.code()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Attach a path to a standard library error.
pub fn io_error(path: &std::path::Path, error: std::io::Error) -> Error {
    Error::io(format!("{}: {error}", path.display()))
}
