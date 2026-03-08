//! DOM overlay: album info display and click-to-start

use vvw_core::project::AlbumMetadata;
use wasm_bindgen::prelude::*;

/// Populate the overlay with album metadata
pub fn populate_album_info(album: &AlbumMetadata) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };

    if let Some(el) = document.get_element_by_id("album-title") {
        let title = if album.title.is_empty() {
            "VVW Player"
        } else {
            &album.title
        };
        el.set_text_content(Some(title));
    }

    if let Some(el) = document.get_element_by_id("album-artist")
        && !album.artist.is_empty()
    {
        el.set_text_content(Some(&album.artist));
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
