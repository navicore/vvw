//! Assembly logic: copy WASM player + album manifests into deploy directory
//!
//! Audio files are NOT included — they go to R2 via the `upload-audio` command.
//! The player discovers the R2 URL from `_config.json`.

use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use vvw_core::project::{AlbumMetadata, ProjectManifest};

use crate::{project_dir, safe_album_path, trunk_build};

/// Assemble the deploy directory with the WASM player and album manifests (no audio).
pub fn assemble(
    workspace_root: &Path,
    albums: &[String],
    output: &Path,
    audio_base_url: Option<&str>,
    site_url: Option<&str>,
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
        let ron_path = src.join("project.ron");
        anyhow::ensure!(
            ron_path.exists(),
            "Project '{}' not found at {}",
            album,
            src.display()
        );

        // Parse manifest once — used for both the copy and OG tag injection
        let ron_text = std::fs::read_to_string(&ron_path)
            .with_context(|| format!("Failed to read project.ron for '{album}'"))?;
        let manifest: ProjectManifest = ron::from_str(&ron_text)
            .with_context(|| format!("Failed to parse project.ron for '{album}'"))?;

        let album_out = safe_album_path(output, album)?;
        std::fs::create_dir_all(&album_out)?;

        // Copy project.ron
        std::fs::copy(&ron_path, album_out.join("project.ron"))
            .with_context(|| format!("Failed to copy project.ron for '{album}'"))?;

        // Inject OG meta tags into index.html from album metadata
        let album_html = inject_og_tags(
            &template_html,
            &manifest.album,
            album,
            audio_base_url,
            site_url,
        )?;
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

/// Inject OG meta tags into `index.html` from album metadata.
fn inject_og_tags(
    template: &str,
    meta: &AlbumMetadata,
    album: &str,
    audio_base_url: Option<&str>,
    site_url: Option<&str>,
) -> Result<String> {
    let title_text = if meta.artist.is_empty() {
        meta.title.clone()
    } else {
        format!("{} — {}", meta.title, meta.artist)
    };

    let escaped_title = html_escape(&title_text);
    let escaped_desc = html_escape(&meta.description);
    let encoded_album = percent_encode(album);

    let mut tags = String::new();
    writeln!(
        tags,
        "    <meta property=\"og:title\" content=\"{escaped_title}\">"
    )
    .unwrap();
    if !meta.description.is_empty() {
        writeln!(
            tags,
            "    <meta property=\"og:description\" content=\"{escaped_desc}\">"
        )
        .unwrap();
    }
    writeln!(tags, "    <meta property=\"og:type\" content=\"website\">").unwrap();

    if let Some(base) = site_url {
        anyhow::ensure!(
            base.starts_with("https://") || base.starts_with("http://"),
            "site_url must use http:// or https:// scheme, got: {base}"
        );
        let base = base.trim_end_matches('/');
        let escaped_url = html_escape(&format!("{base}/{encoded_album}/"));
        writeln!(
            tags,
            "    <meta property=\"og:url\" content=\"{escaped_url}\">"
        )
        .unwrap();
    }

    // Resolve cover art URL: only emit og:image when the result is absolute
    // (OG crawlers ignore relative URLs). In local-preview mode (no audio_base_url),
    // we skip the image tag entirely.
    let resolved_cover = meta
        .cover_art_url
        .as_deref()
        .map(|cover| resolve_url(cover, album, audio_base_url));
    let has_absolute_cover = resolved_cover
        .as_ref()
        .is_some_and(|u| u.starts_with("http://") || u.starts_with("https://"));

    if has_absolute_cover {
        let resolved = html_escape(resolved_cover.as_ref().unwrap());
        writeln!(
            tags,
            "    <meta property=\"og:image\" content=\"{resolved}\">"
        )
        .unwrap();
        writeln!(
            tags,
            "    <meta name=\"twitter:card\" content=\"summary_large_image\">"
        )
        .unwrap();
    } else {
        writeln!(tags, "    <meta name=\"twitter:card\" content=\"summary\">").unwrap();
    }

    // Duplicate title/description for Twitter
    writeln!(
        tags,
        "    <meta name=\"twitter:title\" content=\"{escaped_title}\">"
    )
    .unwrap();
    if !meta.description.is_empty() {
        writeln!(
            tags,
            "    <meta name=\"twitter:description\" content=\"{escaped_desc}\">"
        )
        .unwrap();
    }

    // Insert tags before </head>
    anyhow::ensure!(
        template.contains("</head>"),
        "Could not find </head> in template for album '{album}'. \
         Has the Trunk template changed?"
    );
    let mut html = template.replacen("</head>", &format!("{tags}</head>"), 1);

    // Replace <title>
    anyhow::ensure!(
        html.contains("<title>VVW Player</title>"),
        "Could not find <title>VVW Player</title> in template for album '{album}'. \
         Has the Trunk template title changed?"
    );
    let new_title = format!("<title>{}</title>", html_escape(&title_text));
    html = html.replacen("<title>VVW Player</title>", &new_title, 1);

    Ok(html)
}

/// Resolve a potentially relative URL against the audio base URL.
///
/// Relative cover art lives on R2 at `{base}/{album}/audio/{filename}`,
/// matching the key structure used by `upload-audio`.
fn resolve_url(url: &str, album: &str, audio_base_url: Option<&str>) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if let Some(base) = audio_base_url {
        let base = base.trim_end_matches('/');
        let encoded_album = percent_encode(album);
        let encoded_file = percent_encode(url);
        format!("{base}/{encoded_album}/audio/{encoded_file}")
    } else {
        format!("audio/{}", percent_encode(url))
    }
}

/// Percent-encode a URL path segment (RFC 3986 unreserved characters pass through).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            write!(out, "%{b:02X}").unwrap();
        }
    }
    out
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
