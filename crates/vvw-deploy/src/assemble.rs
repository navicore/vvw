//! Assembly logic: copy WASM player + album manifests into deploy directory
//!
//! Audio files are NOT included — they go to R2 via the `upload-audio` command.
//! The player discovers the R2 URL from `_config.json`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::{project_dir, safe_album_path, trunk_build};

/// Assemble the deploy directory with the WASM player and album manifests (no audio).
pub fn assemble(
    workspace_root: &Path,
    albums: &[String],
    output: &Path,
    audio_base_url: Option<&str>,
) -> Result<()> {
    let dist = trunk_build::dist_dir(workspace_root);
    anyhow::ensure!(
        dist.join("index.html").exists(),
        "Trunk dist not found at {}. Run trunk build first.",
        dist.display()
    );

    // Wipe the output directory to prevent stale files from previous
    // builds or preview/remote mode switches from interfering.
    if output.exists() {
        std::fs::remove_dir_all(output)
            .with_context(|| format!("Failed to clean output dir {}", output.display()))?;
    }
    std::fs::create_dir_all(output)?;

    // No root index.html — only album subdirs get one.
    // A root index.html acts as Cloudflare Pages' SPA catch-all,
    // swallowing requests for project.ron and _config.json.

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

    // Copy each album's project.ron (no audio — that goes to R2)
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

        // Copy index.html into album dir so /AlbumName/ serves the SPA
        std::fs::copy(dist.join("index.html"), album_out.join("index.html"))
            .with_context(|| format!("Failed to copy index.html for '{album}'"))?;

        if audio_base_url.is_none() {
            // Local preview: copy audio files into deploy dir
            let audio_src = src.join("audio");
            if audio_src.exists() {
                let audio_dst = album_out.join("audio");
                std::fs::create_dir_all(&audio_dst)?;
                copy_dir_contents(&audio_src, &audio_dst)
                    .with_context(|| format!("Failed to copy audio for '{album}'"))?;
            }
        }

        println!("  + {album}");
    }

    // Write _config.json with R2 audio URL (only for remote deploy)
    if let Some(url) = audio_base_url {
        let config = format!("{{\"audio_base_url\":\"{url}\"}}");
        std::fs::write(output.join("_config.json"), config)
            .context("Failed to write _config.json")?;
        println!("  Audio URL: {url}");
    }

    // Write Cloudflare Pages _headers for MIME types and caching.
    // WASM/JS are content-hashed (immutable). Config and manifests revalidate
    // every load to prevent stale edge-cache from breaking audio after redeploy.
    std::fs::write(
        output.join("_headers"),
        "\
/*.wasm\n  Content-Type: application/wasm\n  Cache-Control: public, max-age=31536000, immutable\n\
\n/*.js\n  Cache-Control: public, max-age=31536000, immutable\n\
\n/_config.json\n  Cache-Control: no-cache\n\
\n/*/project.ron\n  Cache-Control: no-cache\n\
\n/*/index.html\n  Cache-Control: no-cache\n",
    )
    .context("Failed to write _headers")?;

    // Write _routes.json to prevent SPA fallback for static assets.
    // Without this, Cloudflare Pages serves index.html for .ron, .json, .audio etc.
    let routes = r#"{"version":1,"include":["/*"],"exclude":["/*.js","/*.wasm","/_config.json","/_headers","/*/project.ron","/*/audio/*"]}"#;
    std::fs::write(output.join("_routes.json"), routes).context("Failed to write _routes.json")?;

    Ok(())
}

/// Recursively copy all files from `src` into `dst` (both must be directories).
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            copy_dir_contents(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
