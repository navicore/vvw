//! DOM overlay, header, and track info foldout

use vvw_core::project::{AlbumMetadata, TrackEntry};
use wasm_bindgen::prelude::*;

/// Resolve an image URL: absolute URLs pass through, relative ones are
/// prefixed with the audio base URL (images live alongside audio on R2).
fn resolve_image_url(url: &str, audio_base_url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{audio_base_url}{url}")
    }
}

/// Populate the start overlay and gameplay header with album metadata
pub fn populate_album_info(album: &AlbumMetadata, audio_base_url: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };

    let title = if album.title.is_empty() {
        "VVW Player"
    } else {
        &album.title
    };

    set_text(&document, "album-title", title);
    set_text(&document, "header-title", title);

    if !album.artist.is_empty() {
        set_text(&document, "album-artist", &album.artist);
        set_text(&document, "header-artist", &album.artist);
    }

    if !album.description.is_empty() {
        set_text(&document, "header-description", &album.description);
    }

    // Inject album detail data into hidden #album-data div
    if let Some(container) = document.get_element_by_id("album-data") {
        container
            .set_attribute("data-description", &album.description)
            .ok();
        if let Some(ref url) = album.cover_art_url {
            let resolved = resolve_image_url(url, audio_base_url);
            container
                .set_attribute("data-cover-art-url", &resolved)
                .ok();
        }
        if !album.links.is_empty() {
            let links_json: Vec<String> = album
                .links
                .iter()
                .map(|(label, url)| {
                    format!(
                        "[\"{}\",\"{}\"]",
                        label.replace('\\', "\\\\").replace('"', "\\\""),
                        url.replace('\\', "\\\\").replace('"', "\\\"")
                    )
                })
                .collect();
            container
                .set_attribute("data-links", &format!("[{}]", links_json.join(",")))
                .ok();
        }
    }
}

/// Inject track metadata into the DOM as data attributes on a hidden element.
/// The track-select event handler reads this to populate the foldout.
pub fn inject_track_metadata(tracks: &[TrackEntry], audio_base_url: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(container) = document.get_element_by_id("track-data") else {
        return;
    };

    for entry in tracks {
        let Ok(el) = document.create_element("div") else {
            continue;
        };
        el.set_attribute("data-track-id", &entry.track_id.to_string())
            .ok();
        el.set_attribute("data-title", &entry.metadata.title).ok();
        el.set_attribute("data-artist", &entry.metadata.artist).ok();
        el.set_attribute("data-description", &entry.metadata.description)
            .ok();
        if let Some(ref lyrics) = entry.metadata.lyrics {
            el.set_attribute("data-lyrics", lyrics).ok();
        }
        if let Some(ref url) = entry.metadata.artwork_url {
            let resolved = resolve_image_url(url, audio_base_url);
            el.set_attribute("data-artwork-url", &resolved).ok();
        }
        // Encode links as JSON array of [label, url] pairs
        if !entry.metadata.links.is_empty() {
            let links_json: Vec<String> = entry
                .metadata
                .links
                .iter()
                .map(|(label, url)| {
                    format!(
                        "[\"{}\",\"{}\"]",
                        label.replace('\\', "\\\\").replace('"', "\\\""),
                        url.replace('\\', "\\\\").replace('"', "\\\"")
                    )
                })
                .collect();
            let json = format!("[{}]", links_json.join(","));
            el.set_attribute("data-links", &json).ok();
        }
        container.append_child(&el).ok();
    }
}

/// Dispatch a custom DOM event to show track info in the foldout
pub fn dispatch_track_select(track_id: usize) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };

    let init = web_sys::CustomEventInit::new();
    init.set_detail(&JsValue::from(track_id.to_string()));

    if let Ok(event) = web_sys::CustomEvent::new_with_event_init_dict("track-select", &init) {
        let _ = document.dispatch_event(&event);
    }
}

/// Dispatch a custom DOM event to hide the track info foldout
pub fn dispatch_track_hide() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };

    if let Ok(event) = web_sys::Event::new("track-hide") {
        let _ = document.dispatch_event(&event);
    }
}

/// Set the build datetime in the header
pub fn set_build_info() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    set_text(
        &document,
        "header-build",
        &format!("build {}", env!("VVW_BUILD_DATETIME")),
    );
}

/// Show the album header bar
pub fn show_header() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    if let Some(el) = document.get_element_by_id("album-header")
        && let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = html_el.style().set_property("display", "flex");
    }
}

/// Hide the overlay element
pub fn hide_overlay() -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    if let Some(el) = document.get_element_by_id("overlay") {
        let html_el: web_sys::HtmlElement = el.dyn_into()?;
        html_el.style().set_property("display", "none")?;
    }
    Ok(())
}

fn set_text(document: &web_sys::Document, id: &str, text: &str) {
    if let Some(el) = document.get_element_by_id(id) {
        el.set_text_content(Some(text));
    }
}
