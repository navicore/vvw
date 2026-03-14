//! VVW Deploy CLI — assemble and deploy the WASM player to Cloudflare Pages + R2

mod assemble;
mod create;
mod trunk_build;

use std::collections::HashMap;
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

    /// Export the maze layout as a PNG mask image for artwork creation.
    /// White = corridors, dark = walls. Use as a layer mask in GIMP/Photoshop.
    ExportMaze {
        /// Album name (from saved projects)
        album: String,

        /// Output PNG file path
        #[arg(long, short, default_value = "maze-mask.png")]
        output: PathBuf,

        /// Pixels per tile (higher = more detail for the artist)
        #[arg(long, default_value = "16")]
        scale: u32,
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

/// Local upload manifest: tracks which files have been successfully uploaded to R2.
/// Stored as `.r2-uploaded` in the project's audio directory.
/// Each line is `filename:size` for files confirmed uploaded.
fn load_upload_manifest(audio_dir: &Path) -> HashMap<String, u64> {
    let manifest_path = audio_dir.join(".r2-uploaded");
    let Ok(content) = std::fs::read_to_string(&manifest_path) else {
        return HashMap::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let (name, size_str) = line.rsplit_once(':')?;
            let size = size_str.parse().ok()?;
            Some((name.to_string(), size))
        })
        .collect()
}

fn save_upload_manifest(audio_dir: &Path, manifest: &HashMap<String, u64>) -> Result<()> {
    let manifest_path = audio_dir.join(".r2-uploaded");
    let mut lines: Vec<String> = manifest
        .iter()
        .map(|(name, size)| format!("{name}:{size}"))
        .collect();
    lines.sort();
    std::fs::write(&manifest_path, lines.join("\n"))?;
    Ok(())
}

/// Upload a single file to R2 via wrangler, with retries for transient failures.
fn r2_put(bucket: &str, key: &str, file: &Path) -> Result<()> {
    const MAX_RETRIES: u32 = 3;
    for attempt in 1..=MAX_RETRIES {
        let status = std::process::Command::new("wrangler")
            .args([
                "r2",
                "object",
                "put",
                &format!("{bucket}/{key}"),
                "--file",
                &file.to_string_lossy(),
                "--content-type",
                "audio/flac",
                "--remote",
            ])
            .status()?;
        if status.success() {
            return Ok(());
        }
        if attempt < MAX_RETRIES {
            let wait = attempt * 2;
            eprintln!("  Retry {attempt}/{MAX_RETRIES} for {key} (waiting {wait}s)...");
            std::thread::sleep(std::time::Duration::from_secs(u64::from(wait)));
        } else {
            anyhow::bail!("wrangler r2 object put failed for {key} after {MAX_RETRIES} attempts");
        }
    }
    unreachable!()
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

        let mut manifest = load_upload_manifest(&audio_dir);
        let mut uploaded = 0;
        let mut skipped = 0;

        for entry in std::fs::read_dir(&audio_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let filename = entry.file_name();
            let name = filename.to_string_lossy().to_string();
            if name == ".r2-uploaded" {
                continue;
            }

            let local_size = entry.metadata()?.len();

            // Skip if already uploaded with the same size
            if manifest.get(&name) == Some(&local_size) {
                println!("  Skipping {album}/audio/{name} (already uploaded, {local_size} bytes)");
                skipped += 1;
                continue;
            }

            let key = format!("{album}/audio/{name}");
            print!("  Uploading {key}...");
            r2_put(bucket, &key, &entry.path())?;
            println!(" ok");
            uploaded += 1;

            // Record successful upload
            manifest.insert(name, local_size);
            // Save after each file so partial uploads are tracked
            save_upload_manifest(&audio_dir, &manifest)?;
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

/// Export the maze as a PNG mask image.
/// White pixels = corridors (walkable), dark pixels = walls.
fn cmd_export_maze(album: &str, output: &Path, scale: u32) -> Result<()> {
    use anyhow::Context;
    use image::{GrayImage, Luma};
    use vvw_core::project::ProjectManifest;

    let manifest_path = project_dir(album).join("project.ron");
    anyhow::ensure!(
        manifest_path.exists(),
        "No project.ron for album '{album}' at {}",
        manifest_path.display()
    );

    let ron_str = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: ProjectManifest =
        ron::from_str(&ron_str).with_context(|| format!("parsing {}", manifest_path.display()))?;

    let maze = &manifest.maze;
    anyhow::ensure!(scale >= 1, "scale must be at least 1");
    anyhow::ensure!(
        maze.width > 0 && maze.height > 0,
        "maze has zero dimensions"
    );
    let img_w = (maze.width as u32)
        .checked_mul(scale)
        .ok_or_else(|| anyhow::anyhow!("scale too large: image width overflows u32"))?;
    let img_h = (maze.height as u32)
        .checked_mul(scale)
        .ok_or_else(|| anyhow::anyhow!("scale too large: image height overflows u32"))?;

    let mut img = GrayImage::new(img_w, img_h);

    for ty in 0..maze.height {
        for tx in 0..maze.width {
            let is_wall = maze.is_wall(tx as i32, ty as i32);
            let brightness: u8 = if is_wall { 30 } else { 240 };

            // Fill the scale×scale block for this tile.
            // Flip Y so the image matches the game's visual orientation
            // (maze y=0 is bottom in-game, but pixel y=0 is top in PNG).
            let pixel_y = (maze.height - 1 - ty) as u32 * scale;
            let pixel_x = tx as u32 * scale;
            for dy in 0..scale {
                for dx in 0..scale {
                    img.put_pixel(pixel_x + dx, pixel_y + dy, Luma([brightness]));
                }
            }
        }
    }

    img.save(output)
        .with_context(|| format!("writing {}", output.display()))?;

    println!(
        "Exported maze mask: {}×{} tiles, {}×{} pixels, scale {}x",
        maze.width, maze.height, img_w, img_h, scale
    );
    println!("  → {}", output.display());
    println!();
    println!("Open in GIMP/Photoshop as a layer mask over your artwork.");
    println!("White = corridors (walkable), dark = walls.");

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
            audio_base_url,
        } => {
            let album_names = resolve_albums(albums, all)?;
            let workspace_root = trunk_build::find_workspace_root()?;
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
                    "--branch",
                    "main",
                    "--commit-dirty=true",
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

        Commands::ExportMaze {
            album,
            output,
            scale,
        } => {
            cmd_export_maze(&album, &output, scale)?;
        }

        Commands::DeleteAudio { album, bucket } => {
            cmd_delete_r2_audio(&album, &bucket)?;
        }
    }

    Ok(())
}
