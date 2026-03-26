//! Album creation: scan audio files, collect metadata via editor, generate maze, write project.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use vvw_core::lighting::LightingConfig;
use vvw_core::mazegen::{MazeGenConfig, generate_initial_maze, grow_maze};
use vvw_core::physics::PhysicsConfig;
use vvw_core::project::{AlbumMetadata, ProjectManifest, TrackEntry, TrackMetadata};

use crate::projects_dir;

/// Audio file extensions we recognize.
const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac"];

/// Image file extensions we recognize for artwork.
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];

/// Input metadata format — what the user edits. Distinct from `ProjectManifest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMetadata {
    pub album: InputAlbumMetadata,
    pub tracks: Vec<InputTrackMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputAlbumMetadata {
    pub title: String,
    pub artist: String,
    pub description: String,
    #[serde(default)]
    pub cover_art_url: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub links: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputTrackMetadata {
    pub filename: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub duration_secs: Option<f32>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub lyrics: Option<String>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub links: Vec<(String, String)>,
}

/// Options for the `create` subcommand.
pub struct CreateOptions {
    pub audio_dir: PathBuf,
    pub metadata_file: Option<PathBuf>,
    pub name: Option<String>,
    /// Pre-populate the editor template with this artist name.
    pub artist: Option<String>,
    /// Pre-populate the editor template with this album title.
    pub album_title: Option<String>,
    /// Pre-populate the editor template with this album description.
    pub description: Option<String>,
    /// When `None`, maze config is auto-scaled based on the number of tracks.
    pub maze_config: Option<MazeGenConfig>,
}

/// Run the create-album workflow.
#[allow(clippy::too_many_lines)]
pub fn create_album(opts: &CreateOptions) -> Result<()> {
    // 1. Scan audio files
    let audio_files = scan_audio_files(&opts.audio_dir)?;
    if audio_files.is_empty() {
        anyhow::bail!(
            "No audio files found in {} (looked for: {})",
            opts.audio_dir.display(),
            AUDIO_EXTENSIONS.join(", ")
        );
    }
    println!("Found {} audio file(s)", audio_files.len());

    // 2. Get metadata — from file or via editor (with optional pre-populated values)
    let metadata = if let Some(ref path) = opts.metadata_file {
        load_metadata_file(path)?
    } else {
        edit_metadata(
            &audio_files,
            opts.artist.as_deref(),
            opts.album_title.as_deref(),
            opts.description.as_deref(),
        )?
    };

    // 3. Validate
    validate_metadata(&metadata, &audio_files)?;

    // 4. Derive project name
    let project_name = opts
        .name
        .clone()
        .unwrap_or_else(|| slugify(&metadata.album.title));
    if project_name.is_empty() {
        anyhow::bail!("Could not derive project name from album title — use --name");
    }

    // Sanitize: reject path traversal in --name
    let base = projects_dir();
    std::fs::create_dir_all(&base)
        .with_context(|| format!("creating projects dir: {}", base.display()))?;
    let _ = crate::safe_album_path(&base, &project_name)?;

    // 5. Set up project directory
    let project_dir = base.join(&project_name);
    let audio_out = project_dir.join("audio");
    std::fs::create_dir_all(&audio_out)
        .with_context(|| format!("creating project dir: {}", project_dir.display()))?;

    // 6. Generate maze (auto-scale config to track count when not overridden)
    let maze_config = opts
        .maze_config
        .clone()
        .unwrap_or_else(|| vvw_core::mazegen::config_for_track_count(audio_files.len()));
    let (mut maze, mut state) = generate_initial_maze(&maze_config);
    for i in 0..audio_files.len() {
        grow_maze(&mut maze, &mut state, i);
    }

    // 7. Scan for images (cover art + per-track artwork)
    // Skip auto-detected cover if the user already set an explicit cover_art_url
    let cover_image = find_cover_image(&opts.audio_dir);
    let mut cover_art_filename: Option<String> = None;
    if metadata.album.cover_art_url.is_none()
        && let Some(ref img_path) = cover_image
    {
        let ext = img_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let dest_name = format!("cover.{ext}");
        let dest = audio_out.join(&dest_name);
        std::fs::copy(img_path, &dest).with_context(|| {
            format!(
                "copying cover art {} → {}",
                img_path.display(),
                dest.display()
            )
        })?;
        cover_art_filename = Some(dest_name);
        println!(
            "  Cover art: {}",
            img_path.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    // 8. Build track entries, copy audio files, and match track artwork
    let mut tracks = Vec::with_capacity(audio_files.len());
    for (i, audio_file) in audio_files.iter().enumerate() {
        let dest = audio_out.join(format!("{i}.audio"));
        std::fs::copy(audio_file, &dest)
            .with_context(|| format!("copying {} → {}", audio_file.display(), dest.display()))?;

        let filename = audio_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let track_meta = metadata
            .tracks
            .iter()
            .find(|t| t.filename == filename)
            .with_context(|| {
                format!(
                    "no metadata entry for '{filename}' (file may have been renamed after scan)"
                )
            })?;

        // Check for per-track artwork image (e.g., "Song.jpg" next to "Song.flac")
        // Skip if the matched image is the same as the album cover
        let artwork_url = if let Some(ref url) = track_meta.artwork_url {
            Some(url.clone())
        } else if let Some(img_path) = find_track_image(&opts.audio_dir, audio_file)
            .filter(|p| cover_image.as_ref() != Some(p))
        {
            let ext = img_path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let dest_name = format!("{i}.{ext}");
            let img_dest = audio_out.join(&dest_name);
            std::fs::copy(&img_path, &img_dest).with_context(|| {
                format!(
                    "copying track art {} → {}",
                    img_path.display(),
                    img_dest.display()
                )
            })?;
            println!(
                "  Track art: {} → {dest_name}",
                img_path.file_name().unwrap_or_default().to_string_lossy()
            );
            Some(dest_name)
        } else {
            None
        };

        tracks.push(TrackEntry {
            track_id: i,
            original_filename: filename,
            metadata: TrackMetadata {
                title: track_meta.title.clone(),
                artist: track_meta.artist.clone(),
                duration_secs: track_meta.duration_secs,
                description: track_meta.description.clone().unwrap_or_default(),
                lyrics: track_meta.lyrics.clone(),
                artwork_url,
                links: track_meta.links.clone(),
            },
        });
    }

    // 9. Build and write manifest
    let cover_art_url = metadata.album.cover_art_url.or(cover_art_filename);
    let manifest = ProjectManifest {
        maze,
        rooms: state.rooms,
        maze_config,
        lighting: LightingConfig::default(),
        physics: PhysicsConfig::default(),
        tracks,
        album: AlbumMetadata {
            title: metadata.album.title,
            artist: metadata.album.artist,
            description: metadata.album.description,
            cover_art_url,
            release_date: metadata.album.release_date,
            background_url: None,
            links: metadata.album.links,
            sound_visuals: false,
            mock_feature1: false,
            mock_feature2: false,
            sound_piping: false,
            breadcrumbs: false,
            morph_3d: false,
            wall_walking: false,
        },
    };

    let ron_string = ron::ser::to_string_pretty(&manifest, ron::ser::PrettyConfig::default())
        .context("serializing project manifest")?;
    let manifest_path = project_dir.join("project.ron");
    std::fs::write(&manifest_path, &ron_string)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    // 10. Summary
    println!();
    println!("Album created:");
    println!("  Name:   {project_name}");
    println!("  Title:  {}", manifest.album.title);
    println!("  Artist: {}", manifest.album.artist);
    println!("  Tracks: {}", manifest.tracks.len());
    println!("  Maze:   {}×{}", manifest.maze.width, manifest.maze.height);
    println!("  Path:   {}", project_dir.display());

    Ok(())
}

/// Scan a directory for audio files, sorted alphabetically.
fn scan_audio_files(dir: &Path) -> Result<Vec<PathBuf>> {
    anyhow::ensure!(dir.is_dir(), "'{}' is not a directory", dir.display());

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| {
            let entry = e.ok()?;
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let ext = path.extension()?.to_str()?.to_lowercase();
            if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    files.sort();
    Ok(files)
}

/// Load metadata from a RON file.
fn load_metadata_file(path: &Path) -> Result<InputMetadata> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading metadata file: {}", path.display()))?;
    let metadata: InputMetadata =
        ron::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(metadata)
}

/// Generate a template, open it in `$EDITOR`, and parse the result.
fn edit_metadata(
    audio_files: &[PathBuf],
    artist: Option<&str>,
    album_title: Option<&str>,
    description: Option<&str>,
) -> Result<InputMetadata> {
    let template = build_template(audio_files, artist, album_title, description);

    // Write to a temp file
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join("vvw-album-metadata.ron");
    std::fs::write(&tmp_path, &template)?;

    // Resolve editor — supports args like "emacs -nw" or "code --wait"
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let parts: Vec<&str> = editor.split_whitespace().collect();
    let (cmd, extra_args) = parts.split_first().context("EDITOR is empty")?;

    // Open editor
    let status = std::process::Command::new(cmd)
        .args(extra_args)
        .arg(&tmp_path)
        .status()
        .with_context(|| format!("launching editor: {editor}"))?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    // Read back and strip comment lines
    let raw = std::fs::read_to_string(&tmp_path)?;
    let _ = std::fs::remove_file(&tmp_path);

    let cleaned: String = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    if cleaned.trim().is_empty() {
        anyhow::bail!("Aborting: metadata file is empty (nothing was saved)");
    }

    let metadata: InputMetadata =
        ron::from_str(&cleaned).context("parsing metadata from editor")?;
    Ok(metadata)
}

/// Build the RON template string with comment hints.
///
/// When `artist` or `album_title` are provided, they pre-populate the template.
/// Track titles default to the filename without its extension.
fn build_template(
    audio_files: &[PathBuf],
    artist: Option<&str>,
    album_title: Option<&str>,
    description: Option<&str>,
) -> String {
    use std::fmt::Write;

    let artist_val = artist.unwrap_or_default();
    let title_val = album_title.unwrap_or_default();
    let default_desc;
    let desc_val = match description {
        Some(d) => d,
        None if !title_val.is_empty() => {
            default_desc = format!("{title_val} Album");
            &default_desc
        }
        None => "",
    };
    let escaped_artist = artist_val.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_title = title_val.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_desc = desc_val.replace('\\', "\\\\").replace('"', "\\\"");

    let mut s = String::new();
    s.push_str("// Album metadata — fill in the required fields, then save and quit.\n");
    s.push_str("// Lines starting with // are stripped before parsing.\n");
    s.push_str("(\n");
    s.push_str("    album: (\n");
    let _ = writeln!(s, "        title: \"{escaped_title}\",        // REQUIRED");
    let _ = writeln!(s, "        artist: \"{escaped_artist}\",       // REQUIRED");
    let _ = writeln!(s, "        description: \"{escaped_desc}\",  // REQUIRED");
    s.push_str("        // cover_art_url: None,\n");
    s.push_str("        // release_date: None,\n");
    s.push_str("        // links: [],\n");
    s.push_str("    ),\n");
    s.push_str("    tracks: [\n");
    s.push_str("        // One entry per audio file found. title and artist are REQUIRED.\n");

    for file in audio_files {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        let escaped_name = name.replace('\\', "\\\\").replace('"', "\\\"");
        let stem = file.file_stem().unwrap_or_default().to_string_lossy();
        let escaped_stem = stem.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(
            s,
            "        ( filename: \"{escaped_name}\", title: \"{escaped_stem}\", artist: \"{escaped_artist}\" ),"
        );
    }

    s.push_str("    ],\n");
    s.push_str(")\n");
    s
}

/// Validate that all required fields are present and every audio file has metadata.
fn validate_metadata(metadata: &InputMetadata, audio_files: &[PathBuf]) -> Result<()> {
    let mut errors = Vec::new();

    if metadata.album.title.trim().is_empty() {
        errors.push("album.title is required".to_string());
    }
    if metadata.album.artist.trim().is_empty() {
        errors.push("album.artist is required".to_string());
    }
    if metadata.album.description.trim().is_empty() {
        errors.push("album.description is required".to_string());
    }

    // Check every audio file has a matching track entry
    for file in audio_files {
        let filename = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        match metadata.tracks.iter().find(|t| t.filename == filename) {
            None => errors.push(format!("no track entry for audio file: {filename}")),
            Some(t) => {
                if t.title.trim().is_empty() {
                    errors.push(format!("track '{filename}': title is required"));
                }
                if t.artist.trim().is_empty() {
                    errors.push(format!("track '{filename}': artist is required"));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let msg = errors.join("\n  ");
        anyhow::bail!("Metadata validation failed:\n  {msg}");
    }
}

/// Find the cover art image in a directory (cover.jpg, cover.png, etc.)
fn find_cover_image(dir: &Path) -> Option<PathBuf> {
    for ext in IMAGE_EXTENSIONS {
        let path = dir.join(format!("cover.{ext}"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Find a track artwork image matching an audio file's stem (e.g., "Song.jpg" for "Song.flac")
fn find_track_image(dir: &Path, audio_file: &Path) -> Option<PathBuf> {
    let stem = audio_file.file_stem()?.to_str()?;
    for ext in IMAGE_EXTENSIONS {
        let path = dir.join(format!("{stem}.{ext}"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Simple slug: lowercase, replace non-alphanumeric runs with hyphens, trim hyphens.
fn slugify(s: &str) -> String {
    let slug: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                '-'
            }
        })
        .collect();
    // Collapse runs of hyphens
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    result.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("My Cool Album"), "my-cool-album");
        assert_eq!(slugify("Cognology"), "cognology");
        assert_eq!(slugify("  Spaces  &  Stuff!! "), "spaces-stuff");
    }

    #[test]
    fn template_includes_filenames() {
        let files = vec![
            PathBuf::from("/tmp/01 First.flac"),
            PathBuf::from("/tmp/02 Second.flac"),
        ];
        let template = build_template(
            &files,
            Some("Test Artist"),
            Some("Test Album"),
            Some("A great album"),
        );
        assert!(template.contains("01 First.flac"));
        assert!(template.contains("02 Second.flac"));
        assert!(template.contains("REQUIRED"));
        // Pre-populated values
        assert!(template.contains("Test Artist"));
        assert!(template.contains("Test Album"));
        // Track titles from filename stems
        assert!(template.contains("01 First"));
        assert!(template.contains("02 Second"));
    }

    #[test]
    fn validate_catches_empty_title() {
        let meta = InputMetadata {
            album: InputAlbumMetadata {
                title: String::new(),
                artist: "Ed".to_string(),
                description: "Desc".to_string(),
                cover_art_url: None,
                release_date: None,
                links: vec![],
            },
            tracks: vec![InputTrackMetadata {
                filename: "song.flac".to_string(),
                title: "Song".to_string(),
                artist: "Ed".to_string(),
                duration_secs: None,
                description: None,
                lyrics: None,
                artwork_url: None,
                links: vec![],
            }],
        };
        let files = vec![PathBuf::from("/tmp/song.flac")];
        let result = validate_metadata(&meta, &files);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("album.title is required"));
    }

    #[test]
    fn validate_catches_missing_track() {
        let meta = InputMetadata {
            album: InputAlbumMetadata {
                title: "Album".to_string(),
                artist: "Ed".to_string(),
                description: "Desc".to_string(),
                cover_art_url: None,
                release_date: None,
                links: vec![],
            },
            tracks: vec![],
        };
        let files = vec![PathBuf::from("/tmp/song.flac")];
        let result = validate_metadata(&meta, &files);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no track entry for audio file: song.flac"));
    }

    #[test]
    fn validate_passes_with_complete_metadata() {
        let meta = InputMetadata {
            album: InputAlbumMetadata {
                title: "Album".to_string(),
                artist: "Ed".to_string(),
                description: "Desc".to_string(),
                cover_art_url: None,
                release_date: None,
                links: vec![],
            },
            tracks: vec![InputTrackMetadata {
                filename: "song.flac".to_string(),
                title: "Song Title".to_string(),
                artist: "Ed".to_string(),
                duration_secs: None,
                description: None,
                lyrics: None,
                artwork_url: None,
                links: vec![],
            }],
        };
        let files = vec![PathBuf::from("/tmp/song.flac")];
        assert!(validate_metadata(&meta, &files).is_ok());
    }
}
