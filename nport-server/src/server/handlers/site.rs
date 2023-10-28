use axum::{extract::State, response::IntoResponse};

use crate::server::build_info::BuildInfo;
use crate::server::state::SharedState;
use crate::server::Result;

pub async fn home(State(state): State<SharedState>) -> Result<impl IntoResponse> {
    state.templates().renderer("index.html").render_response()
}

pub async fn build_info() -> impl IntoResponse {
    let build_info = BuildInfo::default();
    serde_json::to_string_pretty(&build_info).unwrap()
}
