//! VVW Deploy CLI — assemble and deploy the WASM player to Cloudflare Pages + R2

mod assemble;
mod create;
mod trunk_build;

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};

/// VVW deploy tool: package and deploy the WASM web player
#[derive(Parser)]
#[command(name = "vvw-deploy", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List saved desktop projects
    List,

    /// Assemble WASM player + album manifests into a deploy directory.
    /// When --audio-base-url is set, audio files are NOT copied (they go to R2).
    /// Without it, audio files are included for local preview.
    Assemble {
        /// Album name(s) to include (from saved desktop projects)
        #[arg(required_unless_present = "all")]
        albums: Vec<String>,

        /// Include all saved projects
        #[arg(long)]
        all: bool,

        /// Output directory (default: ./deploy)
        #[arg(long, short, default_value = "deploy")]
        output: PathBuf,

        /// Force rebuild even if dist/ is newer than sources
        #[arg(long)]
        rebuild: bool,

        /// R2 public URL for audio files (e.g. `https://pub-xxx.r2.dev`).
        /// When set, audio files are excluded from the deploy directory
        /// and a _config.json is written pointing the player to R2.
        #[arg(long)]
        audio_base_url: Option<String>,
    },

    /// Upload audio files to Cloudflare R2
    UploadAudio {
        /// Album name(s) to upload
        #[arg(required_unless_present = "all")]
        albums: Vec<String>,

        /// Upload all saved projects
        #[arg(long)]
        all: bool,

        /// R2 bucket name
        #[arg(long, default_value = "vvw-audio")]
        bucket: String,
    },

    /// Run a local preview server via wrangler (includes audio for local testing)
    Preview {
        /// Deploy directory to serve (default: ./deploy)
        #[arg(long, short, default_value = "deploy")]
        output: PathBuf,
    },

    /// Deploy the player shell to Cloudflare Pages (audio served from R2)
    Deploy {
        /// Deploy directory (default: ./deploy)
        #[arg(long, short, default_value = "deploy")]
        output: PathBuf,

        /// Cloudflare Pages project name
        #[arg(long)]
        project: String,
    },

    /// Create a new album from a directory of audio files.
    /// Opens $EDITOR with a metadata template (like `git commit`).
    /// Use --metadata to skip the editor and load from a file instead.
    /// Use --artist + --album-name to skip the editor with filename-based track titles.
    Create {
        /// Directory containing audio files (wav/mp3/ogg/flac)
        audio_dir: PathBuf,

        /// Load metadata from a RON file instead of opening an editor
        #[arg(long)]
        metadata: Option<PathBuf>,

        /// Artist name — pre-populates the editor template
        #[arg(long)]
        artist: Option<String>,

        /// Album title — pre-populates the editor template
        #[arg(long)]
        album_name: Option<String>,

        /// Album description
        #[arg(long)]
        description: Option<String>,

        /// Project name (default: derived from album title)
        #[arg(long)]
        name: Option<String>,

        /// Minimum room size for maze generation (auto-scaled if omitted)
        #[arg(long)]
        room_size_min: Option<usize>,

        /// Maximum room size for maze generation (auto-scaled if omitted)
        #[arg(long)]
        room_size_max: Option<usize>,

        /// Minimum corridor length for maze generation (auto-scaled if omitted)
        #[arg(long)]
        corridor_length_min: Option<usize>,

        /// Maximum corridor length for maze generation (auto-scaled if omitted)
        #[arg(long)]
        corridor_length_max: Option<usize>,
    },

    /// Remove an album's subdirectory from the deploy directory
    Clean {
        /// Album name to remove
        album: String,

        /// Deploy directory (default: ./deploy)
        #[arg(long, short, default_value = "deploy")]
        output: PathBuf,
    },

    /// Delete an album's audio files from Cloudflare R2
    DeleteAudio {
        /// Album name whose audio to delete
        album: String,

        /// R2 bucket name
        #[arg(long, default_value = "vvw-audio")]
        bucket: String,
    },
}

/// Returns the base directory where all projects are stored.
///
/// Same logic as `vvw-game`'s `project::projects_dir()`:
/// - macOS: `~/Library/Application Support/vvw/projects/`
/// - Linux: `~/.local/share/vvw/projects/`
/// - Windows: `%APPDATA%/vvw/projects/`
fn projects_dir() -> PathBuf {
    dirs::data_dir().map_or_else(
        || PathBuf::from("./vvw-projects"),
        |d| d.join("vvw").join("projects"),
    )
}

/// Returns the directory for a specific named project.
fn project_dir(name: &str) -> PathBuf {
    projects_dir().join(name)
}

/// List saved project names by scanning the projects directory.
fn list_projects() -> Vec<String> {
    let base = projects_dir();
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| {
            let entry = e.ok()?;
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let has_manifest = entry.path().join("project.ron").exists();
            if has_manifest {
                entry.file_name().to_str().map(String::from)
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

/// Validate that an album name resolves inside the output directory.
/// Prevents path traversal via names like `../../etc`.
pub fn safe_album_path(output: &Path, album: &str) -> Result<PathBuf> {
    // Reject empty, dot-only, path separators, or parent references
    let trimmed = album.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        anyhow::bail!(
            "Invalid album name: '{album}' (must be a plain directory name without path separators, '.', or '..')"
        );
    }

    let joined = output.join(trimmed);

    // Double-check via canonicalization when the output dir already exists
    if output.exists() {
        let canonical_output = output.canonicalize()?;
        let canonical_album = joined.canonicalize().unwrap_or_else(|_| {
            // If the album dir doesn't exist yet, canonicalize the parent
            canonical_output.join(trimmed)
        });
        anyhow::ensure!(
            canonical_album.starts_with(&canonical_output),
            "Album path '{}' resolves outside the output directory",
            canonical_album.display()
        );
    }

    Ok(joined)
}

/// Resolve album names from CLI args (explicit list or --all).
fn resolve_albums(albums: Vec<String>, all: bool) -> Result<Vec<String>> {
    if all {
        let names = list_projects();
        if names.is_empty() {
            anyhow::bail!("No saved projects found");
        }
        Ok(names)
    } else {
        Ok(albums)
    }
}

/// Check remote R2 object size via `wrangler r2 object head`.
/// Returns `Some(size)` if the object exists, `None` if it doesn't.
fn r2_head(bucket: &str, key: &str) -> Result<Option<u64>> {
    let output = std::process::Command::new("wrangler")
        .args([
            "r2",
            "object",
            "head",
            &format!("{bucket}/{key}"),
            "--remote",
        ])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    // Parse contentLength from the JSON-ish output
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("\"contentLength\":") {
            let size_str = rest.trim().trim_end_matches(',');
            if let Ok(size) = size_str.parse::<u64>() {
                return Ok(Some(size));
            }
        }
        // Also handle "contentLength": 12345 (with space after colon)
        if let Some(rest) = trimmed.strip_prefix("contentLength:") {
            let size_str = rest.trim().trim_end_matches(',');
            if let Ok(size) = size_str.parse::<u64>() {
                return Ok(Some(size));
            }
        }
    }
    // Object exists but we couldn't parse size — treat as missing to force re-upload
    Ok(None)
}

/// Upload a single file to R2 via wrangler.
fn r2_put(bucket: &str, key: &str, file: &Path) -> Result<()> {
    let status = std::process::Command::new("wrangler")
        .args([
            "r2",
            "object",
            "put",
            &format!("{bucket}/{key}"),
            "--file",
            &file.to_string_lossy(),
            "--remote",
        ])
        .status()?;
    if !status.success() {
        anyhow::bail!("wrangler r2 object put failed for {key}");
    }
    Ok(())
}

fn cmd_upload_audio(album_names: &[String], bucket: &str) -> Result<()> {
    for album in album_names {
        let src = project_dir(album);
        let audio_dir = src.join("audio");
        anyhow::ensure!(
            audio_dir.exists(),
            "No audio directory for '{}' at {}",
            album,
            audio_dir.display()
        );

        let mut uploaded = 0;
        let mut skipped = 0;
        for entry in std::fs::read_dir(&audio_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let local_size = entry.metadata()?.len();
            let filename = entry.file_name();
            let key = format!("{album}/audio/{}", filename.to_string_lossy());

            if let Some(remote_size) = r2_head(bucket, &key)? {
                if remote_size == local_size {
                    println!("  Skipping {key} (already uploaded, {local_size} bytes)");
                    skipped += 1;
                    continue;
                }
                println!(
                    "  Re-uploading {key} (size mismatch: local {local_size} vs remote {remote_size})"
                );
            } else {
                print!("  Uploading {key}...");
            }

            r2_put(bucket, &key, &entry.path())?;
            println!(" ok");
            uploaded += 1;
        }
        println!(
            "  + {album}: {uploaded} uploaded, {skipped} skipped (already in R2 bucket '{bucket}')"
        );
    }
    Ok(())
}

/// Delete a single object from R2 via wrangler.
fn r2_delete(bucket: &str, key: &str) -> Result<()> {
    let status = std::process::Command::new("wrangler")
        .args([
            "r2",
            "object",
            "delete",
            &format!("{bucket}/{key}"),
            "--remote",
        ])
        .status()?;
    if !status.success() {
        anyhow::bail!("wrangler r2 object delete failed for {key}");
    }
    Ok(())
}

/// Delete an album's audio files from R2 by reading the local project to discover keys.
fn cmd_delete_r2_audio(album: &str, bucket: &str) -> Result<()> {
    let src = project_dir(album);
    let audio_dir = src.join("audio");
    anyhow::ensure!(
        audio_dir.exists(),
        "No local audio directory for '{}' at {} — cannot determine R2 keys to delete",
        album,
        audio_dir.display()
    );

    let mut count = 0;
    for entry in std::fs::read_dir(&audio_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let filename = entry.file_name();
        let key = format!("{album}/audio/{}", filename.to_string_lossy());
        print!("  Deleting {key}...");
        r2_delete(bucket, &key)?;
        println!(" ok");
        count += 1;
    }
    println!("Deleted {count} audio file(s) from R2 bucket '{bucket}'");
    Ok(())
}

fn cmd_wrangler(args: &[&str], label: &str) -> Result<()> {
    let status = std::process::Command::new("wrangler").args(args).status()?;
    if !status.success() {
        anyhow::bail!("{label}: wrangler exited with {status}");
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => {
            let projects = list_projects();
            if projects.is_empty() {
                println!("No saved projects found in {}", projects_dir().display());
            } else {
                println!("Saved projects:");
                for name in &projects {
                    println!("  {name}");
                }
            }
        }

        Commands::Assemble {
            albums,
            all,
            output,
            rebuild,
            audio_base_url,
        } => {
            let album_names = resolve_albums(albums, all)?;
            let workspace_root = trunk_build::find_workspace_root()?;
            trunk_build::build_wasm(&workspace_root, rebuild)?;
            assemble::assemble(
                &workspace_root,
                &album_names,
                &output,
                audio_base_url.as_deref(),
            )?;
            println!(
                "Assembled {} album(s) into {}",
                album_names.len(),
                output.display()
            );
        }

        Commands::UploadAudio {
            albums,
            all,
            bucket,
        } => {
            let album_names = resolve_albums(albums, all)?;
            cmd_upload_audio(&album_names, &bucket)?;
        }

        Commands::Preview { output } => {
            anyhow::ensure!(
                output.exists(),
                "Deploy directory not found: {}",
                output.display()
            );
            println!("Starting local preview server...");
            cmd_wrangler(&["pages", "dev", &output.to_string_lossy()], "preview")?;
        }

        Commands::Deploy { output, project } => {
            anyhow::ensure!(
                output.exists(),
                "Deploy directory not found: {}",
                output.display()
            );
            println!("Deploying to Cloudflare Pages project '{project}'...");
            cmd_wrangler(
                &[
                    "pages",
                    "deploy",
                    &output.to_string_lossy(),
                    "--project-name",
                    &project,
                ],
                "deploy",
            )?;
        }

        Commands::Create {
            audio_dir,
            metadata,
            artist,
            album_name,
            description,
            name,
            room_size_min,
            room_size_max,
            corridor_length_min,
            corridor_length_max,
        } => {
            // Only build explicit config if any maze args were provided;
            // otherwise let create_album auto-scale based on track count.
            let maze_config = if room_size_min.is_some()
                || room_size_max.is_some()
                || corridor_length_min.is_some()
                || corridor_length_max.is_some()
            {
                let r_min = room_size_min.unwrap_or(3);
                let r_max = room_size_max.unwrap_or(7);
                let c_min = corridor_length_min.unwrap_or(4);
                let c_max = corridor_length_max.unwrap_or(8);
                anyhow::ensure!(
                    r_min <= r_max,
                    "--room-size-min ({r_min}) must be <= --room-size-max ({r_max})"
                );
                anyhow::ensure!(
                    c_min <= c_max,
                    "--corridor-length-min ({c_min}) must be <= --corridor-length-max ({c_max})"
                );
                Some(vvw_core::mazegen::MazeGenConfig {
                    min_room_size: r_min,
                    max_room_size: r_max,
                    min_corridor_length: c_min,
                    max_corridor_length: c_max,
                    ..Default::default()
                })
            } else {
                None
            };
            create::create_album(&create::CreateOptions {
                audio_dir,
                metadata_file: metadata,
                artist,
                album_title: album_name,
                description,
                name,
                maze_config,
            })?;
        }

        Commands::Clean { album, output } => {
            let album_dir = safe_album_path(&output, &album)?;
            if album_dir.exists() {
                std::fs::remove_dir_all(&album_dir)?;
                println!("Removed {}", album_dir.display());
            } else {
                println!("Album directory not found: {}", album_dir.display());
            }
        }

        Commands::DeleteAudio { album, bucket } => {
            cmd_delete_r2_audio(&album, &bucket)?;
        }
    }

    Ok(())
}
