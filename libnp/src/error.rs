use serde_json;
use thiserror;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("serde_json `{0}`")]
    SerdeJSON(serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::SerdeJSON(err)
    }
}
