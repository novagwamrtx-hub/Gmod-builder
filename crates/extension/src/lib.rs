//! Content script for Steam's Workshop pages, using the session of the
//! logged-in user. Nothing is sent anywhere but Steam.
//!
//! - On a collection's "manage" page it shows a panel that brings the
//!   collection in line with a list of addons (see `manage`).
//! - On an addon's page it adds a button that collects addons in a basket
//!   for that panel (see `basket`).

mod basket;
pub mod logic;
mod manage;

use gmcb_core::{handoff, parse_workshop_id};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, Element, HtmlElement, Window};

const BASKET_KEY: &str = "gmcb_basket";

pub(crate) const PANEL_STYLE: &str = "position:fixed;right:16px;bottom:16px;z-index:10000;\
    width:320px;padding:14px;background:#1b1e25;color:#dce0e8;border:1px solid #2e3440;\
    border-radius:8px;font:13px/1.4 Arial,sans-serif;box-shadow:0 6px 24px rgba(0,0,0,.6)";
pub(crate) const BUTTON_STYLE: &str = "padding:6px 12px;margin:0 6px 6px 0;background:#4a9eff;\
    color:#081424;border:0;border-radius:6px;cursor:pointer;font-weight:bold";
pub(crate) const QUIET_BUTTON_STYLE: &str = "padding:6px 12px;margin:0 6px 6px 0;\
    background:#2b303b;color:#dce0e8;border:1px solid #2e3440;border-radius:6px;cursor:pointer";
pub(crate) const MUTED_STYLE: &str = "color:#8a93a3;margin-top:4px";

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let location = window.location();
    let Some(id) = parse_workshop_id(&location.search()?) else {
        return Ok(());
    };
    if location.pathname()?.contains("filedetails") {
        return basket::show(window, document, id);
    }
    let handed_over = handoff::decode_fragment(&location.hash()?);
    manage::show(window, document, id, handed_over)
}

/// Creates an element and appends it to `parent`.
pub(crate) fn child(
    document: &Document,
    parent: &Element,
    tag: &str,
    style: &str,
    text: &str,
) -> Result<Element, JsValue> {
    let el = document.create_element(tag)?;
    if !style.is_empty() {
        el.set_attribute("style", style)?;
    }
    if !text.is_empty() {
        el.set_text_content(Some(text));
    }
    parent.append_child(&el)?;
    Ok(el)
}

pub(crate) fn button(
    document: &Document,
    parent: &Element,
    style: &str,
    text: &str,
) -> Result<HtmlElement, JsValue> {
    child(document, parent, "button", style, text)?.dyn_into().map_err(Into::into)
}

/// Runs `action` whenever `target` is clicked, for the life of the page.
pub(crate) fn on_click(target: &HtmlElement, action: impl FnMut() + 'static) {
    let closure = Closure::<dyn FnMut()>::new(action);
    target.set_onclick(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
}

/// Ids of the elements matching `selector`, read as Workshop ids.
pub(crate) fn ids_on_page(document: &Document, selector: &str) -> Vec<u64> {
    let mut ids = Vec::new();
    if let Ok(nodes) = document.query_selector_all(selector) {
        for i in 0..nodes.length() {
            let id = nodes
                .item(i)
                .and_then(|node| node.dyn_into::<Element>().ok())
                .and_then(|el| logic::id_from_element_id(&el.id()));
            ids.extend(id.filter(|id| !ids.contains(id)));
        }
    }
    ids
}

pub(crate) fn read_basket(window: &Window) -> Vec<u64> {
    window
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item(BASKET_KEY).ok().flatten())
        .map(|stored| logic::basket_from_storage(&stored))
        .unwrap_or_default()
}

pub(crate) fn write_basket(window: &Window, ids: &[u64]) {
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item(BASKET_KEY, &logic::basket_to_storage(ids));
    }
}

/// Copies text to the clipboard. Must be called from a click.
pub(crate) fn copy_text(window: &Window, text: &str) -> bool {
    let call = || -> Result<(), JsValue> {
        let navigator = js_sys::Reflect::get(window, &"navigator".into())?;
        let clipboard = js_sys::Reflect::get(&navigator, &"clipboard".into())?;
        let write: js_sys::Function = js_sys::Reflect::get(&clipboard, &"writeText".into())?.dyn_into()?;
        write.call1(&clipboard, &text.into())?;
        Ok(())
    };
    call().is_ok()
}

pub(crate) async fn post_form(window: &Window, url: &str, body: &str) -> Result<String, JsValue> {
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(body));
    let request = web_sys::Request::new_with_str_and_init(url, &init)?;
    request.headers().set("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")?;
    let response: web_sys::Response =
        JsFuture::from(window.fetch_with_request(&request)).await?.dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str("request failed"));
    }
    Ok(JsFuture::from(response.text()?).await?.as_string().unwrap_or_default())
}

pub(crate) async fn sleep(window: &Window, ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(promise).await;
}
