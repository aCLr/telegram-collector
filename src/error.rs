use rtdlib::errors::RTDError;
use std::fmt;

#[derive(Debug)]
pub struct Error {
    message: String,
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<RTDError> for Error {
    fn from(err: RTDError) -> Self {
        Self {
            message: err.to_string(),
        }
    }
}
