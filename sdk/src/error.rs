use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

/// Why reading assets failed; see docs/features/sdk.md#errors.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// Bytes that are not what the spec says. `file` names the format (`amesh.bin`) or,
    /// from [`crate::Assets::load`], the asset's path.
    Format { file: String, message: String },
    /// An ID or path the assets do not hold.
    NotFound(String),
}

impl Error {
    pub(crate) fn format(file: &str, message: impl Into<String>) -> Self {
        Self::Format {
            file: file.to_owned(),
            message: message.into(),
        }
    }

    /// The same error, naming `file` instead of the format.
    pub(crate) fn in_file(self, file: &str) -> Self {
        match self {
            Self::Format { message, .. } => Self::format(file, message),
            other => other,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Format { file, message } => write!(f, "{file}: {message}"),
            Self::NotFound(what) => write!(f, "{what} not found"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
