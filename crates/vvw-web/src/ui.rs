//! DOM overlay, header, and track info foldout

use vvw_core::project::{AlbumMetadata, TrackEntry};
use wasm_bindgen::prelude::*;

/// Populate the start overlay and gameplay header with album metadata
pub fn populate_album_info(album: &AlbumMetadata) {
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
}

/// Inject track metadata into the DOM as data attributes on a hidden element.
/// The track-select event handler reads this to populate the foldout.
pub fn inject_track_metadata(tracks: &[TrackEntry]) {
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
