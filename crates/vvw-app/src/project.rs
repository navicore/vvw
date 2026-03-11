//! Project persistence: save and load maze projects to/from disk
//!
//! A project directory contains:
//! - `project.ron` — serialized manifest (maze, configs, track metadata)
//! - `audio/` — raw audio files keyed by track ID

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use vvw_core::project::AlbumMetadata;
pub use vvw_core::project::{ProjectManifest, TrackEntry};

use vvw_core::lighting::LightingConfig;
use vvw_core::mazegen::MazeGenState;
use vvw_game::Maze;

use crate::admin::TrackAudioFile;

/// Resource holding the project name from the CLI `--project` arg.
/// When set, the named project is loaded on startup.
#[derive(Resource)]
pub struct StartupProject(pub Option<String>);

/// Returns the base directory where all projects are stored.
///
/// - macOS: `~/Library/Application Support/vvw/projects/`
/// - Linux: `~/.local/share/vvw/projects/`
/// - Windows: `%APPDATA%/vvw/projects/`
///
/// Falls back to `./vvw-projects/` if the platform data dir can't be determined.
pub fn projects_dir() -> PathBuf {
    dirs::data_dir().map_or_else(
        || PathBuf::from("./vvw-projects"),
        |d| d.join("vvw").join("projects"),
    )
}

/// Returns the directory for a specific named project.
/// Sanitizes the name to prevent path traversal and platform-specific issues.
pub fn project_dir(name: &str) -> PathBuf {
    // Windows reserved device names
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    let trimmed = name.trim();
    let sanitized: String = trimmed
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == '\0' || c == ':' {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Reject empty, whitespace-only, and pure-dot names like "." and ".."
    let mut sanitized = if sanitized.trim().is_empty() || sanitized.trim_matches('.').is_empty() {
        "unnamed".to_string()
    } else {
        sanitized
    };
    // Strip trailing dots and spaces (problematic on Windows)
    sanitized = sanitized.trim_end_matches(['.', ' ']).to_string();
    if sanitized.is_empty() {
        sanitized = "unnamed".to_string();
    }
    if RESERVED.contains(&sanitized.to_uppercase().as_str()) {
        sanitized = format!("project_{sanitized}");
    }
    projects_dir().join(sanitized)
}

/// List saved project names by scanning the projects directory.
/// Returns an empty vec if the directory doesn't exist yet.
pub fn list_projects() -> Vec<String> {
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
            // Only include directories that contain a project.ron
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

/// Errors that can occur during project save/load
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialize(#[from] ron::Error),
    #[error("deserialization error: {0}")]
    Deserialize(#[from] ron::error::SpannedError),
}

/// Save the current game state to a project directory.
#[allow(clippy::implicit_hasher)]
pub fn save_project(
    path: &Path,
    maze: &Maze,
    gen_state: &MazeGenState,
    lighting: &LightingConfig,
    physics: &vvw_core::physics::PhysicsConfig,
    track_audio: &HashMap<usize, TrackAudioFile>,
    album: &AlbumMetadata,
) -> Result<(), ProjectError> {
    // Create directories
    let audio_dir = path.join("audio");
    std::fs::create_dir_all(&audio_dir)?;

    // Write audio files
    for (track_id, audio_file) in track_audio {
        let audio_path = audio_dir.join(format!("{track_id}.audio"));
        std::fs::write(&audio_path, &audio_file.bytes)?;
    }

    // Build track entries
    let tracks: Vec<TrackEntry> = track_audio
        .iter()
        .map(|(track_id, audio_file)| TrackEntry {
            track_id: *track_id,
            original_filename: audio_file.original_filename.clone(),
            metadata: audio_file.metadata.clone(),
        })
        .collect();

    // Build manifest
    let manifest = ProjectManifest {
        maze: maze.clone(),
        rooms: gen_state.rooms.clone(),
        maze_config: gen_state.config.clone(),
        lighting: lighting.clone(),
        physics: physics.clone(),
        tracks,
        album: album.clone(),
    };

    // Serialize and write
    let ron_string = ron::ser::to_string_pretty(&manifest, ron::ser::PrettyConfig::default())?;
    std::fs::write(path.join("project.ron"), ron_string)?;

    Ok(())
}

/// Load a project from a directory, returning the manifest and audio bytes.
pub fn load_project(
    path: &Path,
) -> Result<(ProjectManifest, HashMap<usize, Vec<u8>>), ProjectError> {
    // Read and deserialize manifest
    let ron_string = std::fs::read_to_string(path.join("project.ron"))?;
    let manifest: ProjectManifest = ron::from_str(&ron_string)?;

    // Read audio files
    let audio_dir = path.join("audio");
    let mut audio_bytes = HashMap::new();
    for entry in &manifest.tracks {
        let audio_path = audio_dir.join(format!("{}.audio", entry.track_id));
        let bytes = std::fs::read(&audio_path)?;
        audio_bytes.insert(entry.track_id, bytes);
    }

    Ok((manifest, audio_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vvw_core::mazegen::{MazeGenConfig, generate_initial_maze};
    use vvw_core::project::TrackMetadata;

    #[test]
    fn round_trip_empty_project() {
        let config = MazeGenConfig::default();
        let (maze, state) = generate_initial_maze(&config);
        let lighting = LightingConfig::default();
        let track_audio = HashMap::new();

        let dir = std::env::temp_dir().join("vvw_test_empty_project");
        let _ = std::fs::remove_dir_all(&dir);

        let album = AlbumMetadata::default();
        let physics = vvw_core::physics::PhysicsConfig::default();
        save_project(
            &dir,
            &maze,
            &state,
            &lighting,
            &physics,
            &track_audio,
            &album,
        )
        .unwrap();
        let (loaded_manifest, loaded_audio) = load_project(&dir).unwrap();

        assert_eq!(loaded_manifest.maze.width, maze.width);
        assert_eq!(loaded_manifest.maze.height, maze.height);
        assert_eq!(loaded_manifest.rooms.len(), state.rooms.len());
        assert!(loaded_audio.is_empty());
        assert!(loaded_manifest.tracks.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trip_with_tracks() {
        let config = MazeGenConfig::default();
        let (maze, state) = generate_initial_maze(&config);
        let lighting = LightingConfig::default();

        let mut track_audio = HashMap::new();
        track_audio.insert(
            0,
            TrackAudioFile {
                original_filename: "song.mp3".to_string(),
                bytes: vec![0xFF, 0xFB, 0x90, 0x00], // fake mp3 header
                metadata: TrackMetadata::default(),
            },
        );
        track_audio.insert(
            1,
            TrackAudioFile {
                original_filename: "beat.wav".to_string(),
                bytes: vec![0x52, 0x49, 0x46, 0x46], // "RIFF"
                metadata: TrackMetadata::default(),
            },
        );

        let dir = std::env::temp_dir().join("vvw_test_tracks_project");
        let _ = std::fs::remove_dir_all(&dir);

        let album = AlbumMetadata::default();
        let physics = vvw_core::physics::PhysicsConfig::default();
        save_project(
            &dir,
            &maze,
            &state,
            &lighting,
            &physics,
            &track_audio,
            &album,
        )
        .unwrap();
        let (loaded_manifest, loaded_audio) = load_project(&dir).unwrap();

        assert_eq!(loaded_manifest.tracks.len(), 2);
        assert_eq!(loaded_audio.len(), 2);
        assert_eq!(loaded_audio[&0], vec![0xFF, 0xFB, 0x90, 0x00]);
        assert_eq!(loaded_audio[&1], vec![0x52, 0x49, 0x46, 0x46]);

        // Verify filenames preserved
        let track0 = loaded_manifest
            .tracks
            .iter()
            .find(|t| t.track_id == 0)
            .unwrap();
        assert_eq!(track0.original_filename, "song.mp3");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_dir_sanitizes_path_traversal() {
        let base = projects_dir();

        // Slashes replaced with underscores
        assert_eq!(project_dir("../../etc"), base.join(".._.._etc"));
        assert_eq!(project_dir("a/b\\c"), base.join("a_b_c"));

        // Colons replaced with underscores
        assert_eq!(project_dir("C:foo"), base.join("C_foo"));

        // Dot-only names rejected
        assert_eq!(project_dir(".."), base.join("unnamed"));
        assert_eq!(project_dir("."), base.join("unnamed"));

        // Empty and whitespace-only names rejected
        assert_eq!(project_dir(""), base.join("unnamed"));
        assert_eq!(project_dir("   "), base.join("unnamed"));

        // Leading/trailing whitespace trimmed
        assert_eq!(project_dir("  my-maze  "), base.join("my-maze"));

        // Normal names pass through
        assert_eq!(project_dir("my-maze"), base.join("my-maze"));
        assert_eq!(project_dir("cool project"), base.join("cool project"));
    }
}
