//! Errors shared by the command line tools.

use std::path::PathBuf;

use thiserror::Error;

/// A failure that is reported to the user as `error: ...`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("failed to parse font {path}: {source}")]
    ParseFont {
        path: PathBuf,
        #[source]
        source: ttf_parser::FaceParsingError,
    },

    #[error("{path} is a WOFF/WOFF2 file; convert it to .ttf or .otf first")]
    WoffInput { path: PathBuf },

    #[error("{path} is a TrueType collection (.ttc); collections are not supported yet")]
    CollectionInput { path: PathBuf },
}
