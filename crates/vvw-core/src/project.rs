//! Project types — platform-independent data structures for project serialization

use serde::{Deserialize, Serialize};

use crate::lighting::LightingConfig;
use crate::maze::Maze;
use crate::mazegen::{MazeGenConfig, Room};
use crate::physics::PhysicsConfig;

/// Metadata for a single audio track in the project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackEntry {
    pub track_id: usize,
    pub original_filename: String,
    #[serde(default)]
    pub metadata: TrackMetadata,
}

/// Album-level metadata for web publishing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlbumMetadata {
    pub title: String,
    pub artist: String,
    pub description: String,
    pub cover_art_url: Option<String>,
    pub release_date: Option<String>,
    /// Background artwork URL (maze-textured image rendered behind tiles)
    #[serde(default)]
    pub background_url: Option<String>,
    /// External links: (label, url) pairs
    pub links: Vec<(String, String)>,
    /// Enable sound wave visuals (pulsing ellipses radiating from track sources)
    #[serde(default)]
    pub sound_visuals: bool,
}

/// Per-track metadata for web publishing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub duration_secs: Option<f32>,
    pub description: String,
    pub lyrics: Option<String>,
    /// URL to track artwork image (recommended 160x160px+)
    #[serde(default)]
    pub artwork_url: Option<String>,
    /// External links: (label, url) pairs
    pub links: Vec<(String, String)>,
}

/// Serialized project manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub maze: Maze,
    pub rooms: Vec<Room>,
    pub maze_config: MazeGenConfig,
    pub lighting: LightingConfig,
    #[serde(default)]
    pub physics: PhysicsConfig,
    pub tracks: Vec<TrackEntry>,
    #[serde(default)]
    pub album: AlbumMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mazegen::generate_initial_maze;

    /// Simulate a pre-Phase-2 manifest: a RON struct that has no `album`
    /// field on the manifest and no `metadata` field on `TrackEntry`.
    /// `#[serde(default)]` must fill them in with defaults.
    #[test]
    fn deserialize_legacy_manifest_without_metadata() {
        // We define a "legacy" mirror struct without the new fields.
        // Serializing it produces RON that matches what old code wrote.
        #[derive(Debug, Clone, serde::Serialize)]
        struct LegacyTrackEntry {
            track_id: usize,
            original_filename: String,
        }

        #[derive(Debug, Clone, serde::Serialize)]
        struct LegacyManifest {
            maze: Maze,
            rooms: Vec<crate::mazegen::Room>,
            maze_config: MazeGenConfig,
            lighting: LightingConfig,
            tracks: Vec<LegacyTrackEntry>,
        }

        let config = MazeGenConfig::default();
        let (maze, state) = generate_initial_maze(&config);
        let lighting = LightingConfig::default();

        let legacy = LegacyManifest {
            maze,
            rooms: state.rooms,
            maze_config: config,
            lighting,
            tracks: vec![LegacyTrackEntry {
                track_id: 0,
                original_filename: "song.mp3".to_string(),
            }],
        };

        let ron_string = ron::ser::to_string_pretty(&legacy, ron::ser::PrettyConfig::default())
            .expect("serialize legacy manifest");

        let loaded: ProjectManifest =
            ron::from_str(&ron_string).expect("deserialize legacy manifest");
        assert_eq!(loaded.album.title, "");
        assert_eq!(loaded.album.artist, "");
        assert_eq!(loaded.album.description, "");
        assert_eq!(loaded.tracks.len(), 1);
        assert_eq!(loaded.tracks[0].metadata.title, "");
        assert_eq!(loaded.tracks[0].metadata.artist, "");
        assert_eq!(loaded.tracks[0].original_filename, "song.mp3");
    }

    #[test]
    fn round_trip_with_metadata() {
        let config = MazeGenConfig::default();
        let (maze, state) = generate_initial_maze(&config);
        let lighting = LightingConfig::default();

        let manifest = ProjectManifest {
            maze,
            rooms: state.rooms,
            maze_config: config,
            lighting,
            physics: PhysicsConfig::default(),
            tracks: vec![TrackEntry {
                track_id: 0,
                original_filename: "song.mp3".to_string(),
                metadata: TrackMetadata {
                    title: "My Song".to_string(),
                    artist: "Test Artist".to_string(),
                    duration_secs: Some(180.0),
                    description: "A test track".to_string(),
                    lyrics: Some("la la la".to_string()),
                    artwork_url: None,
                    links: vec![("web".to_string(), "https://example.com".to_string())],
                },
            }],
            album: AlbumMetadata {
                title: "My Album".to_string(),
                artist: "Test Artist".to_string(),
                description: "A test album".to_string(),
                cover_art_url: Some("https://example.com/cover.png".to_string()),
                release_date: Some("2025-01-01".to_string()),
                background_url: None,
                links: vec![("bandcamp".to_string(), "https://bc.example.com".to_string())],
                sound_visuals: false,
            },
        };

        let ron_string = ron::ser::to_string_pretty(&manifest, ron::ser::PrettyConfig::default())
            .expect("serialize");
        let loaded: ProjectManifest = ron::from_str(&ron_string).expect("deserialize");

        assert_eq!(loaded.album.title, "My Album");
        assert_eq!(loaded.album.artist, "Test Artist");
        assert_eq!(
            loaded.album.cover_art_url.as_deref(),
            Some("https://example.com/cover.png")
        );
        assert_eq!(loaded.tracks[0].metadata.title, "My Song");
        assert_eq!(loaded.tracks[0].metadata.artist, "Test Artist");
        assert_eq!(loaded.tracks[0].metadata.duration_secs, Some(180.0));
        assert_eq!(
            loaded.tracks[0].metadata.lyrics.as_deref(),
            Some("la la la")
        );
    }
}
