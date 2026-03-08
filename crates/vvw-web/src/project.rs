//! Project loading: fetch project.ron and audio files via the Fetch API

use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

use vvw_core::project::ProjectManifest;

/// A fully loaded project with manifest and decoded audio bytes
pub struct LoadedProject {
    pub manifest: ProjectManifest,
    pub audio_data: HashMap<usize, Vec<u8>>,
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

/// Fetch and parse the project from the same origin
pub async fn load_project() -> Result<LoadedProject, JsValue> {
    let base_path = get_base_path();
    let manifest_text = fetch_text(&format!("{base_path}project.ron")).await?;
    let manifest: ProjectManifest = ron::from_str(&manifest_text)
        .map_err(|e| JsValue::from_str(&format!("RON parse error: {e}")))?;

    let mut audio_data = HashMap::new();
    for entry in &manifest.tracks {
        let url = format!("{base_path}audio/{}.audio", entry.track_id);
        let bytes = fetch_bytes(&url).await?;
        audio_data.insert(entry.track_id, bytes);
    }

    Ok(LoadedProject {
        manifest,
        audio_data,
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

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, JsValue> {
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

    let array_buffer = JsFuture::from(resp.array_buffer()?).await?;
    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    Ok(uint8_array.to_vec())
}
