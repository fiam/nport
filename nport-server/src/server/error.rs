use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("anyhow: {0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("template error: {0}")]
    Template(#[from] tera::Error),
}

pub type Result<T = ()> = std::result::Result<T, Error>;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::Anyhow(inner) => {
                tracing::error!(
                    backtrace = ?inner.backtrace(),
                    error = ?inner,
                    "internal server error"
                );
            }
            Error::Template(inner) => {
                tracing::error!(
                    error = ?inner,
                    "error in template"
                );
            }
        }
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
    }
}
