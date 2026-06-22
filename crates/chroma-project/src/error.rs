//! The project-layer error type.

use std::fmt;

/// Errors from loading, saving, or migrating a project.
///
/// Hand-rolled (no `thiserror`) to keep the dependency surface minimal, matching
/// the contract crates (see `DECISIONS.md`).
#[derive(Debug)]
pub enum ProjectError {
    /// Reading or writing the project file failed.
    Io(String),
    /// The JSON could not be (de)serialized into a [`Project`](chroma_core_api::Project).
    Serde(String),
    /// The file's `version` is newer than this build understands.
    UnsupportedVersion {
        /// The version found in the file.
        found: u32,
        /// The newest version this build can read.
        supported: u32,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::Io(e) => write!(f, "project I/O error: {e}"),
            ProjectError::Serde(e) => write!(f, "project (de)serialization error: {e}"),
            ProjectError::UnsupportedVersion { found, supported } => write!(
                f,
                "project version {found} is newer than supported version {supported}"
            ),
        }
    }
}

impl std::error::Error for ProjectError {}

/// Convenience alias for project operations.
pub type Result<T> = std::result::Result<T, ProjectError>;
