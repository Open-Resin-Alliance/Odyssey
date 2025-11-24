use std::{
    error::Error,
    fmt::{Debug, Display},
    io,
};

use poem::{error::ResponseError, http::StatusCode};
use tokio::{
    sync::{
        broadcast::error::{RecvError, SendError, TryRecvError},
        mpsc::error::{SendError as mpscSendError, TryRecvError as mpscTryRecvError},
    },
    task::JoinError,
};

#[derive(Debug)]
pub struct OdysseyError {
    pub error_type: ErrorType,
    pub source: Box<dyn Error + Send + Sync>,
    pub error_code: u16,
}

#[derive(Debug, Clone)]
pub enum ErrorType {
    HardwareError,
    InternalStateError,
    ConfigurationError,
    PrintError,
    FileError,
}

impl OdysseyError {
    pub fn hardware_error(source: Box<dyn Error + Send + Sync>, error_code: u16) -> OdysseyError {
        OdysseyError::new(ErrorType::HardwareError, source, error_code)
    }
    pub fn internal_state_error(
        source: Box<dyn Error + Send + Sync>,
        error_code: u16,
    ) -> OdysseyError {
        OdysseyError::new(ErrorType::InternalStateError, source, error_code)
    }
    pub fn configuration_error(
        source: Box<dyn Error + Send + Sync>,
        error_code: u16,
    ) -> OdysseyError {
        OdysseyError::new(ErrorType::ConfigurationError, source, error_code)
    }
    pub fn print_error(source: Box<dyn Error + Send + Sync>, error_code: u16) -> OdysseyError {
        OdysseyError::new(ErrorType::PrintError, source, error_code)
    }
    pub fn file_error(source: Box<dyn Error + Send + Sync>, error_code: u16) -> OdysseyError {
        OdysseyError::new(ErrorType::FileError, source, error_code)
    }
    pub fn new(
        error_type: ErrorType,
        source: Box<dyn Error + Send + Sync>,
        error_code: u16,
    ) -> OdysseyError {
        OdysseyError {
            error_type,
            source,
            error_code,
        }
    }
}

impl Error for OdysseyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

impl Display for OdysseyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl ResponseError for OdysseyError {
    fn status(&self) -> poem::http::StatusCode {
        StatusCode::from_u16(self.error_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl From<RecvError> for OdysseyError {
    fn from(err: RecvError) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 500,
        }
    }
}

impl From<TryRecvError> for OdysseyError {
    fn from(err: TryRecvError) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 500,
        }
    }
}
impl<T: Debug + Send + Sync + 'static> From<SendError<T>> for OdysseyError {
    fn from(err: SendError<T>) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 500,
        }
    }
}

impl From<mpscTryRecvError> for OdysseyError {
    fn from(err: mpscTryRecvError) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 500,
        }
    }
}
impl<T: Debug + Send + Sync + 'static> From<mpscSendError<T>> for OdysseyError {
    fn from(err: mpscSendError<T>) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 500,
        }
    }
}
impl From<io::Error> for OdysseyError {
    fn from(err: io::Error) -> OdysseyError {
        let error_code = match err.kind() {
            io::ErrorKind::NotFound => 404,
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => 403,
            io::ErrorKind::AlreadyExists => 409,
            io::ErrorKind::StorageFull => 507,
            io::ErrorKind::FileTooLarge => 413,
            io::ErrorKind::InvalidFilename
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::InvalidData
            | io::ErrorKind::NotADirectory
            | io::ErrorKind::IsADirectory => 400,
            _ => 500,
        };
        OdysseyError {
            error_type: ErrorType::FileError,
            source: Box::new(err),
            error_code,
        }
    }
}
impl From<self_update::errors::Error> for OdysseyError {
    fn from(err: self_update::errors::Error) -> Self {
        OdysseyError {
            error_type: ErrorType::InternalStateError,
            source: Box::new(err),
            error_code: 500,
        }
    }
}
impl From<JoinError> for OdysseyError {
    fn from(err: JoinError) -> Self {
        OdysseyError {
            error_type: ErrorType::InternalStateError,
            source: Box::new(err),
            error_code: 500,
        }
    }
}
