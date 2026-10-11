//! The basket button on an addon's Workshop page: collect addons while
//! browsing Steam, then add them all from a collection's manage page or
//! paste them into the desktop app.
//!
//! The basket lives in the page's local storage, which every
//! steamcommunity.com page shares, so nothing leaves the browser.

use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlElement, Window};

use crate::{button, child, copy_text, logic, on_click, read_basket, write_basket};
use crate::{BUTTON_STYLE, MUTED_STYLE, PANEL_STYLE, QUIET_BUTTON_STYLE};

/// Present on Workshop pages of Garry's Mod items.
const GMOD_MARKER: &str = "a[href*='/app/4000']";
/// Present on a collection's page, which is laid out like an addon's. A
/// collection cannot be put in another, so it gets no basket button.
const COLLECTION_MARKER: &str = ".collectionChildren, .collectionHeader";

#[derive(Clone)]
struct Basket {
    window: Window,
    id: u64,
    toggle: HtmlElement,
    copy: HtmlElement,
    clear: HtmlElement,
    count: Element,
}

pub fn show(window: Window, document: Document, id: u64) -> Result<(), JsValue> {
    if document.query_selector(GMOD_MARKER)?.is_none()
        || document.query_selector(COLLECTION_MARKER)?.is_some()
    {
        return Ok(());
    }
    let body = document.body().ok_or("no body")?;
    let root = child(&document, &body, "div", &format!("{PANEL_STYLE};width:auto"), "")?;
    let basket = Basket {
        toggle: button(&document, &root, BUTTON_STYLE, "")?,
        copy: button(&document, &root, QUIET_BUTTON_STYLE, "Copy ids")?,
        clear: button(&document, &root, QUIET_BUTTON_STYLE, "Empty")?,
        count: child(&document, &root, "div", MUTED_STYLE, "")?,
        window,
        id,
    };
    basket.wire();
    basket.refresh();
    Ok(())
}

impl Basket {
    fn wire(&self) {
        let basket = self.clone();
        on_click(&self.toggle, move || {
            let mut ids = read_basket(&basket.window);
            logic::toggle(&mut ids, basket.id);
            write_basket(&basket.window, &ids);
            basket.refresh();
        });

        let basket = self.clone();
        on_click(&self.copy, move || {
            let ids = read_basket(&basket.window);
            let lines: Vec<String> = ids.iter().map(u64::to_string).collect();
            let note = if !copy_text(&basket.window, &lines.join("\n")) {
                "Could not reach the clipboard.".to_string()
            } else if ids.len() == 1 {
                "Copied 1 id. Paste it into the app's Draft tab.".to_string()
            } else {
                format!("Copied {} ids. Paste them into the app's Draft tab.", ids.len())
            };
            basket.count.set_text_content(Some(&note));
        });

        let basket = self.clone();
        on_click(&self.clear, move || {
            write_basket(&basket.window, &[]);
            basket.refresh();
        });
    }

    fn refresh(&self) {
        let ids = read_basket(&self.window);
        let label = if ids.contains(&self.id) { "✔ In basket" } else { "+ Add to basket" };
        self.toggle.set_text_content(Some(label));
        self.copy.set_hidden(ids.is_empty());
        self.clear.set_hidden(ids.is_empty());
        let count = match ids.len() {
            0 => "Your basket is empty.".to_string(),
            n => format!("{n} in your basket. Open a collection's edit page to add them."),
        };
        self.count.set_text_content(Some(&count));
    }
}
