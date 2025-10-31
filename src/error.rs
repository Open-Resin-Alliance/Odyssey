use std::{
    error::Error,
    fmt::{Debug, Display},
    io,
};

use tokio::sync::broadcast::error::{RecvError, SendError, TryRecvError};

#[derive(Debug)]
pub struct OdysseyError {
    pub error_type: ErrorType,
    pub source: Box<dyn Error + Send + Sync>,
    pub error_code: usize,
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
    pub fn hardware_error(source: Box<dyn Error + Send + Sync>, error_code: usize) -> OdysseyError {
        OdysseyError::new(ErrorType::HardwareError, source, error_code)
    }
    pub fn internal_state_error(
        source: Box<dyn Error + Send + Sync>,
        error_code: usize,
    ) -> OdysseyError {
        OdysseyError::new(ErrorType::InternalStateError, source, error_code)
    }
    pub fn configuration_error(
        source: Box<dyn Error + Send + Sync>,
        error_code: usize,
    ) -> OdysseyError {
        OdysseyError::new(ErrorType::ConfigurationError, source, error_code)
    }
    pub fn print_error(source: Box<dyn Error + Send + Sync>, error_code: usize) -> OdysseyError {
        OdysseyError::new(ErrorType::PrintError, source, error_code)
    }
    pub fn file_error(source: Box<dyn Error + Send + Sync>, error_code: usize) -> OdysseyError {
        OdysseyError::new(ErrorType::FileError, source, error_code)
    }
    pub fn new(
        error_type: ErrorType,
        source: Box<dyn Error + Send + Sync>,
        error_code: usize,
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

impl From<RecvError> for OdysseyError {
    fn from(err: RecvError) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 0,
        }
    }
}

impl From<TryRecvError> for OdysseyError {
    fn from(err: TryRecvError) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 0,
        }
    }
}
impl From<SendError<String>> for OdysseyError {
    fn from(err: SendError<String>) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 0,
        }
    }
}
impl From<io::Error> for OdysseyError {
    fn from(err: io::Error) -> OdysseyError {
        OdysseyError {
            error_type: ErrorType::HardwareError,
            source: Box::new(err),
            error_code: 0,
        }
    }
}
