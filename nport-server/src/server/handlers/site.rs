use axum::http::StatusCode;
use axum::{extract::State, response::IntoResponse};
use serde::Serialize;
use shadow_rs::shadow;

shadow!(build);

use crate::server::state::SharedState;

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct BuildInfo<'a> {
    build_time: &'a str,
    debug: bool,
    branch: String,
    tag: String,
    is_git_clean: bool,
    git_status: String,
    commit_hash: &'a str,
    build_os: &'a str,
    rust_version: &'a str,
    pkg_version: &'a str,
    cargo_tree: &'a str,
}

impl<'a> BuildInfo<'a> {
    pub fn default() -> Self {
        Self {
            build_time: build::BUILD_TIME,
            debug: shadow_rs::is_debug(),
            branch: shadow_rs::branch(),
            tag: shadow_rs::tag(),
            is_git_clean: shadow_rs::git_clean(),
            git_status: shadow_rs::git_status_file(),
            commit_hash: build::COMMIT_HASH,
            build_os: build::BUILD_OS,
            rust_version: build::RUST_VERSION,
            pkg_version: build::PKG_VERSION,
            cargo_tree: build::CARGO_TREE,
        }
    }
}

pub async fn home(State(_state): State<SharedState>) -> impl IntoResponse {
    (StatusCode::OK, "hello")
}

pub async fn build_info() -> impl IntoResponse {
    serde_json::to_string_pretty(&BuildInfo::default()).unwrap()
}
