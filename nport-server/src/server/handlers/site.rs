use axum::{extract::State, response::IntoResponse};
use serde::Serialize;

use crate::server::build_info::BuildInfo;
use crate::server::state::SharedState;
use crate::server::Result;

pub async fn home(State(state): State<SharedState>) -> Result<impl IntoResponse> {
    state.templates().renderer("index.html").render_response()
}

#[derive(Serialize)]
struct StatsContext {
    pub client_count: i64,
    pub http_hostname_count: i64,
    pub tcp_port_count: i64,
}

pub async fn stats(State(state): State<SharedState>) -> Result<impl IntoResponse> {
    let data = StatsContext {
        client_count: state.stats().client_connections().get(),
        http_hostname_count: state.stats().http_hostnames().get(),
        tcp_port_count: state.stats().tcp_ports().get(),
    };
    state
        .templates()
        .renderer("stats.html")
        .with_data(data)
        .render_response()
}

pub async fn build_info() -> impl IntoResponse {
    let build_info = BuildInfo::default();
    serde_json::to_string_pretty(&build_info).unwrap()
}
