//! Trunk build orchestration: find workspace root, locate dist output

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Walk up from the current directory looking for a workspace Cargo.toml
/// that contains `vvw-web` as a member.
pub fn find_workspace_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let contents = std::fs::read_to_string(&cargo_toml)?;
            if contents.contains("vvw-web") && contents.contains("[workspace]") {
                return Ok(dir);
            }
        }
        if !dir.pop() {
            anyhow::bail!(
                "Could not find workspace root containing vvw-web. \
                 Run from within the vvw workspace."
            );
        }
    }
}

/// Returns the path to Trunk's dist directory.
pub fn dist_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("crates/vvw-web/dist")
}
