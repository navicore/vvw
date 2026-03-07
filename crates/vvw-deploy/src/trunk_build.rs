//! Trunk build orchestration: find workspace root, build WASM, cache detection

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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

/// Check whether the Trunk dist/ output is newer than the vvw-web sources.
fn dist_is_fresh(workspace_root: &Path) -> bool {
    let dist_dir = workspace_root.join("crates/vvw-web/dist");
    let index = dist_dir.join("index.html");
    if !index.exists() {
        return false;
    }

    let Ok(dist_meta) = std::fs::metadata(&index) else {
        return false;
    };
    let Ok(dist_mtime) = dist_meta.modified() else {
        return false;
    };

    // Check if any source file in vvw-web/src or vvw-core/src is newer than dist
    let source_dirs = [
        workspace_root.join("crates/vvw-web/src"),
        workspace_root.join("crates/vvw-core/src"),
    ];

    for source_dir in &source_dirs {
        if let Ok(entries) = walkdir(source_dir) {
            for entry in entries {
                if let Ok(meta) = std::fs::metadata(&entry)
                    && let Ok(mtime) = meta.modified()
                    && mtime > dist_mtime
                {
                    return false;
                }
            }
        }
    }

    // Also check config files that affect the build
    let config_files = [
        workspace_root.join("crates/vvw-web/Cargo.toml"),
        workspace_root.join("crates/vvw-web/Trunk.toml"),
        workspace_root.join("crates/vvw-web/index.html"),
        workspace_root.join("crates/vvw-core/Cargo.toml"),
        workspace_root.join("Cargo.toml"),
        workspace_root.join("Cargo.lock"),
    ];

    for path in &config_files {
        if let Ok(meta) = std::fs::metadata(path)
            && let Ok(mtime) = meta.modified()
            && mtime > dist_mtime
        {
            return false;
        }
    }

    true
}

/// Simple recursive file listing (avoids adding walkdir dependency).
fn walkdir(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        } else if file_type.is_dir() {
            files.extend(walkdir(&entry.path())?);
        } else {
            files.push(entry.path());
        }
    }
    Ok(files)
}

/// Build the WASM player with Trunk. Skips if dist/ is fresh (unless `force` is true).
pub fn build_wasm(workspace_root: &Path, force: bool) -> Result<()> {
    if !force && dist_is_fresh(workspace_root) {
        println!("Trunk dist/ is up to date, skipping build (use --rebuild to force)");
        return Ok(());
    }

    let web_crate = workspace_root.join("crates/vvw-web");
    println!("Building WASM player with trunk...");

    let status = std::process::Command::new("trunk")
        .args(["build", "--release"])
        .current_dir(&web_crate)
        .status()
        .context("Failed to run trunk. Is it installed? (cargo install trunk)")?;

    if !status.success() {
        anyhow::bail!("trunk build failed with {status}");
    }

    println!("Trunk build complete");
    Ok(())
}

/// Returns the path to Trunk's dist directory.
pub fn dist_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("crates/vvw-web/dist")
}
