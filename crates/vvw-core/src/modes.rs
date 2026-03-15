//! Interaction mode types — platform-independent identifiers and descriptors

use serde::{Deserialize, Serialize};

/// Identifier for a registered interaction mode. String-based so feature
/// plugins can define their own IDs without modifying this crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModeId(pub String);

/// Descriptor for a registered interaction mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeDescriptor {
    pub id: ModeId,
    pub label: String,
    /// When true, player movement is suppressed while this mode is active.
    /// Most modes (piping, sculpting, recording) allow normal movement.
    pub suppresses_movement: bool,
}
