use axum::http::StatusCode;
use axum::{extract::State, response::IntoResponse};

use crate::server::state::SharedState;

pub async fn home(State(state): State<SharedState>) -> impl IntoResponse {
    return (StatusCode::OK, "hello");
}
