//! Assembly logic: copy WASM player + album manifests into deploy directory
//!
//! Audio files are NOT included — they go to R2 via the `upload-audio` command.
//! The player discovers the R2 URL from `_config.json`.

use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use vvw_core::project::ProjectManifest;

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

    // Read the template index.html once
    let template_html = std::fs::read_to_string(dist.join("index.html"))
        .context("Failed to read dist/index.html")?;

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

        // Inject OG meta tags into index.html from album metadata
        let album_html = inject_og_tags(&template_html, &src, album, audio_base_url)?;
        std::fs::write(album_out.join("index.html"), album_html)
            .with_context(|| format!("Failed to write index.html for '{album}'"))?;

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

/// Read album metadata from `project.ron` and inject OG meta tags into `index.html`.
fn inject_og_tags(
    template: &str,
    project_dir: &Path,
    album: &str,
    audio_base_url: Option<&str>,
) -> Result<String> {
    let ron_text = std::fs::read_to_string(project_dir.join("project.ron"))
        .with_context(|| format!("Failed to read project.ron for '{album}'"))?;
    let manifest: ProjectManifest = ron::from_str(&ron_text)
        .with_context(|| format!("Failed to parse project.ron for '{album}'"))?;

    let meta = &manifest.album;
    let title_text = if meta.artist.is_empty() {
        meta.title.clone()
    } else {
        format!("{} — {}", meta.title, meta.artist)
    };

    let escaped_title = html_escape(&title_text);
    let escaped_desc = html_escape(&meta.description);

    let mut tags = String::new();
    let _ = writeln!(
        tags,
        "    <meta property=\"og:title\" content=\"{escaped_title}\">"
    );
    if !meta.description.is_empty() {
        let _ = writeln!(
            tags,
            "    <meta property=\"og:description\" content=\"{escaped_desc}\">"
        );
    }
    let _ = writeln!(tags, "    <meta property=\"og:type\" content=\"website\">");

    // Resolve cover art URL: relative paths get prefixed with audio_base_url
    if let Some(ref cover) = meta.cover_art_url {
        let resolved = html_escape(&resolve_url(cover, audio_base_url));
        let _ = writeln!(
            tags,
            "    <meta property=\"og:image\" content=\"{resolved}\">"
        );
        let _ = writeln!(
            tags,
            "    <meta name=\"twitter:card\" content=\"summary_large_image\">"
        );
    } else {
        let _ = writeln!(tags, "    <meta name=\"twitter:card\" content=\"summary\">");
    }

    // Duplicate title/description for Twitter
    let _ = writeln!(
        tags,
        "    <meta name=\"twitter:title\" content=\"{escaped_title}\">"
    );
    if !meta.description.is_empty() {
        let _ = writeln!(
            tags,
            "    <meta name=\"twitter:description\" content=\"{escaped_desc}\">"
        );
    }

    // Insert tags before </head> and replace <title>
    let mut html = template.replace("</head>", &format!("{tags}</head>"));
    html = html.replace(
        "<title>VVW Player</title>",
        &format!("<title>{}</title>", html_escape(&title_text)),
    );

    Ok(html)
}

/// Resolve a potentially relative URL against the audio base URL.
fn resolve_url(url: &str, audio_base_url: Option<&str>) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if let Some(base) = audio_base_url {
        format!("{base}{url}")
    } else {
        format!("audio/{url}")
    }
}

/// Escape HTML attribute values.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
