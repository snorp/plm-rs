/// Errors returned from various operations.
#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("I/O error: {0:?}")]
    IoError(::std::io::ErrorKind),

    #[error("Nom error: {0:?}")]
    NomError(nom::error::ErrorKind),

    /// A [Message](super::Message) or modem command was not acknowledged.
    #[error("Command was not acknowledged")]
    NotAcknowledged,

    /// Failure to parse a [Message](super::Message) or modem command.
    #[error("Parse error")]
    Parse,

    /// An operation took too long to complete.
    #[error("Operation timed out")]
    Timeout,

    /// An unexpected response was received.
    #[error("Unexpected response received")]
    UnexpectedResponse,

    /// An invalid [Address](super::Address) string was passed.
    #[error("Invalid address format. Expected 'xx.xx.xx'.")]
    InvalidAddress,

    /// The modem was disconnected.
    #[error("Modem was disconnected.")]
    Disconnected,
}

impl From<::std::io::Error> for Error {
    fn from(e: ::std::io::Error) -> Error {
        Error::IoError(e.kind())
    }
}

impl From<nom::error::ErrorKind> for Error {
    fn from(e: nom::error::ErrorKind) -> Error {
        Error::NomError(e)
    }
}

impl From<futures::channel::mpsc::SendError> for Error {
    fn from(_: futures::channel::mpsc::SendError) -> Error {
        Error::Disconnected
    }
}
