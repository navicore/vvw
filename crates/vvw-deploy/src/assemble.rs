//! Assembly logic: copy WASM player + album data into deploy directory

use std::path::Path;

use anyhow::{Context, Result};

use crate::{project_dir, safe_album_path, trunk_build};

/// Assemble the deploy directory with the WASM player and album data.
pub fn assemble(workspace_root: &Path, albums: &[String], output: &Path) -> Result<()> {
    let dist = trunk_build::dist_dir(workspace_root);
    anyhow::ensure!(
        dist.join("index.html").exists(),
        "Trunk dist not found at {}. Run trunk build first.",
        dist.display()
    );

    std::fs::create_dir_all(output)?;

    // Copy index.html → player.html
    std::fs::copy(dist.join("index.html"), output.join("player.html"))
        .context("Failed to copy index.html as player.html")?;

    // Copy .js and .wasm files to output root
    for entry in std::fs::read_dir(&dist)? {
        let entry = entry?;
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if ext == "js" || ext == "wasm" {
            let out_file = output.join(entry.file_name());
            std::fs::copy(&path, &out_file)
                .with_context(|| format!("Failed to copy {}", path.display()))?;
        }
    }

    // Copy each album's project.ron + audio/ into output/<album>/
    for album in albums {
        let src = project_dir(album);
        anyhow::ensure!(
            src.join("project.ron").exists(),
            "Project '{}' not found at {}",
            album,
            src.display()
        );

        let album_out = safe_album_path(output, album)?;
        std::fs::create_dir_all(&album_out)?;

        // Copy project.ron
        std::fs::copy(src.join("project.ron"), album_out.join("project.ron"))
            .with_context(|| format!("Failed to copy project.ron for '{album}'"))?;

        // Copy audio/ directory
        let audio_src = src.join("audio");
        if audio_src.exists() {
            let audio_dst = album_out.join("audio");
            std::fs::create_dir_all(&audio_dst)?;
            copy_dir_contents(&audio_src, &audio_dst)
                .with_context(|| format!("Failed to copy audio for '{album}'"))?;
        }

        println!("  + {album}");
    }

    // Write Cloudflare Pages _redirects
    std::fs::write(output.join("_redirects"), "/*  /player.html  200\n")
        .context("Failed to write _redirects")?;

    Ok(())
}

/// Recursively copy all files from `src` into `dst` (both must be directories).
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            copy_dir_contents(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
