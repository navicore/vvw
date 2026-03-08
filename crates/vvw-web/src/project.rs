//! Project loading: fetch project.ron manifest and site config via the Fetch API
//!
//! Audio files are streamed by the browser via `<audio>` elements —
//! we only need to fetch the manifest and config here, not the audio bytes.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

use vvw_core::project::ProjectManifest;

/// A loaded project: manifest + base URL for audio files
pub struct LoadedProject {
    pub manifest: ProjectManifest,
    pub audio_base_url: String,
}

/// Derive the base path from `window.location.pathname` so the player
/// works when deployed under a sub-path (e.g. `/cool-album/`).
fn get_base_path() -> String {
    let window = web_sys::window().expect("no window");
    let pathname = window.location().pathname().unwrap_or_else(|_| "/".into());
    if pathname == "/" {
        "/".to_string()
    } else {
        let trimmed = pathname.trim_end_matches('/');
        format!("{trimmed}/")
    }
}

/// Fetch `/_config.json` to get the audio base URL (R2 bucket).
/// Returns empty string if not found (fall back to same-origin).
async fn fetch_audio_base_url() -> String {
    let Ok(text) = fetch_text("/_config.json").await else {
        return String::new();
    };
    // Minimal JSON parsing: look for "audio_base_url":"<value>"
    // Avoids adding serde_json dependency for one field
    parse_audio_base_url(&text).unwrap_or_default()
}

fn parse_audio_base_url(text: &str) -> Option<String> {
    let start = text.find("\"audio_base_url\"")?;
    let rest = &text[start..];
    let colon = rest.find(':')?;
    let after_colon = rest[colon + 1..].trim();
    let inner = after_colon.strip_prefix('"')?;
    let end = inner.find('"')?;
    let url = inner[..end].trim_end_matches('/');
    if url.is_empty() {
        None
    } else {
        Some(format!("{url}/"))
    }
}

/// Fetch and parse the project manifest from the same origin.
/// Audio base URL is resolved from `_config.json` or falls back to same-origin.
pub async fn load_project() -> Result<LoadedProject, JsValue> {
    let base_path = get_base_path();
    let manifest_text = fetch_text(&format!("{base_path}project.ron")).await?;
    let manifest: ProjectManifest = ron::from_str(&manifest_text)
        .map_err(|e| JsValue::from_str(&format!("RON parse error: {e}")))?;

    // Resolve audio URL: R2 bucket if configured, otherwise same-origin
    let r2_base = fetch_audio_base_url().await;
    let audio_base_url = if r2_base.is_empty() {
        // Same-origin: audio is next to project.ron
        format!("{base_path}audio/")
    } else {
        // R2: audio is at <r2_base>/<album_path>/audio/
        // base_path starts with "/" — strip it to avoid double slash
        let album_path = base_path.trim_start_matches('/');
        format!("{r2_base}{album_path}audio/")
    };

    web_sys::console::log_1(&format!("Audio base URL: {audio_base_url}").into());

    Ok(LoadedProject {
        manifest,
        audio_base_url,
    })
}

async fn fetch_text(url: &str) -> Result<String, JsValue> {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::SameOrigin);

    let request = Request::new_with_str_and_init(url, &opts)?;
    let window = web_sys::window().ok_or("no window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;

    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "fetch {url} failed: {}",
            resp.status()
        )));
    }

    let text = JsFuture::from(resp.text()?).await?;
    text.as_string()
        .ok_or_else(|| JsValue::from_str("response not a string"))
}
