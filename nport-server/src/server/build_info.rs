use serde::Serialize;
use shadow_rs::shadow;

shadow!(build);

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct BuildInfo<'a> {
    pub build_time: &'a str,
    pub debug: bool,
    pub branch: String,
    pub tag: String,
    pub is_git_clean: bool,
    pub git_status: String,
    pub commit_hash: &'a str,
    pub short_commit_hash: &'a str,
    pub build_os: &'a str,
    pub rust_version: &'a str,
    pub pkg_version: &'a str,
    pub cargo_tree: &'a str,
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
            short_commit_hash: build::SHORT_COMMIT,
            build_os: build::BUILD_OS,
            rust_version: build::RUST_VERSION,
            pkg_version: build::PKG_VERSION,
            cargo_tree: build::CARGO_TREE,
        }
    }
}
