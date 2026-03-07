//! VVW Deploy CLI — assemble saved desktop projects into a Cloudflare Pages deployment

mod assemble;
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

    /// Assemble WASM player + album data into a deploy directory
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
    },

    /// Run a local preview server via wrangler
    Preview {
        /// Deploy directory to serve (default: ./deploy)
        #[arg(long, short, default_value = "deploy")]
        output: PathBuf,
    },

    /// Deploy to Cloudflare Pages
    Deploy {
        /// Deploy directory (default: ./deploy)
        #[arg(long, short, default_value = "deploy")]
        output: PathBuf,

        /// Cloudflare Pages project name
        #[arg(long)]
        project: String,
    },

    /// Remove an album's subdirectory from the deploy directory
    Clean {
        /// Album name to remove
        album: String,

        /// Deploy directory (default: ./deploy)
        #[arg(long, short, default_value = "deploy")]
        output: PathBuf,
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
    // Reject names containing path separators or parent references
    if album.contains('/') || album.contains('\\') || album.contains("..") {
        anyhow::bail!("Invalid album name: '{album}' (must not contain path separators or '..')");
    }

    let joined = output.join(album);

    // Double-check via canonicalization when the output dir already exists
    if output.exists() {
        let canonical_output = output.canonicalize()?;
        let canonical_album = joined.canonicalize().unwrap_or_else(|_| {
            // If the album dir doesn't exist yet, canonicalize the parent
            canonical_output.join(album)
        });
        anyhow::ensure!(
            canonical_album.starts_with(&canonical_output),
            "Album path '{}' resolves outside the output directory",
            canonical_album.display()
        );
    }

    Ok(joined)
}

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
        } => {
            let album_names = if all {
                let names = list_projects();
                if names.is_empty() {
                    anyhow::bail!("No saved projects found");
                }
                names
            } else {
                albums
            };

            let workspace_root = trunk_build::find_workspace_root()?;
            trunk_build::build_wasm(&workspace_root, rebuild)?;
            assemble::assemble(&workspace_root, &album_names, &output)?;

            println!(
                "Assembled {} album(s) into {}",
                album_names.len(),
                output.display()
            );
        }

        Commands::Preview { output } => {
            anyhow::ensure!(
                output.exists(),
                "Deploy directory not found: {}",
                output.display()
            );
            println!("Starting local preview server...");
            let status = std::process::Command::new("wrangler")
                .args(["pages", "dev", &output.to_string_lossy()])
                .status()?;
            if !status.success() {
                anyhow::bail!("wrangler exited with {status}");
            }
        }

        Commands::Deploy { output, project } => {
            anyhow::ensure!(
                output.exists(),
                "Deploy directory not found: {}",
                output.display()
            );
            println!("Deploying to Cloudflare Pages project '{project}'...");
            let status = std::process::Command::new("wrangler")
                .args([
                    "pages",
                    "deploy",
                    &output.to_string_lossy(),
                    "--project-name",
                    &project,
                ])
                .status()?;
            if !status.success() {
                anyhow::bail!("wrangler exited with {status}");
            }
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
    }

    Ok(())
}
