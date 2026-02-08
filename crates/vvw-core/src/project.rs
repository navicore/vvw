//! Project types — platform-independent data structures for project serialization

use serde::{Deserialize, Serialize};

use crate::lighting::LightingConfig;
use crate::maze::Maze;
use crate::mazegen::{MazeGenConfig, Room};

/// Metadata for a single audio track in the project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackEntry {
    pub track_id: usize,
    pub original_filename: String,
}

/// Album-level metadata for web publishing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlbumMetadata {
    pub title: String,
    pub artist: String,
    pub description: String,
    pub cover_art_url: Option<String>,
    pub release_date: Option<String>,
    /// External links: (label, url) pairs
    pub links: Vec<(String, String)>,
}

/// Per-track metadata for web publishing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub duration_secs: Option<f32>,
    pub description: String,
    pub lyrics: Option<String>,
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
    pub tracks: Vec<TrackEntry>,
}
