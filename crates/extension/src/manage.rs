//! The panel on a collection's "manage" page. It takes a list of addons
//! (from the desktop app's link, the basket, or pasted) and brings the
//! collection in line with it, one request at a time.
//!
//! Steam has no public API for editing collections; `addchild` and
//! `removechild` are the requests the page's own buttons make, and may
//! change without notice.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use gmcb_core::handoff;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Document, Element, HtmlDocument, HtmlElement, HtmlTextAreaElement, Window};

use crate::{button, child, ids_on_page, logic, on_click, post_form, sleep};
use crate::{BUTTON_STYLE, MUTED_STYLE, PANEL_STYLE, QUIET_BUTTON_STYLE};

const ADD_URL: &str = "https://steamcommunity.com/sharedfiles/addchild";
const REMOVE_URL: &str = "https://steamcommunity.com/sharedfiles/removechild";
/// Pause between requests, to stay clear of Steam's rate limiting.
const DELAY_MS: i32 = 1000;
/// Give up after this many failures in a row: something is wrong.
const MAX_FAILURE_STREAK: usize = 3;
/// What the collection holds, and the user's subscribed items, as marked up
/// on the page.
const MEMBERS: &str = "[id^='sharedfile_'], .itemChoice.inCollection";
const SUBSCRIBED: &str = "#MySubscribedItems .itemChoice";

const TEXTAREA_STYLE: &str = "width:100%;height:110px;box-sizing:border-box;margin:8px 0;\
    padding:6px;background:#101216;color:#dce0e8;border:1px solid #2e3440;border-radius:6px;\
    font:12px monospace";
const BAR_STYLE: &str = "height:6px;margin:8px 0;background:#101216;border-radius:3px;overflow:hidden";
const DANGER_BUTTON_STYLE: &str = "padding:6px 12px;margin:0 6px 6px 0;background:#2b303b;\
    color:#ec6a70;border:1px solid #ec6a70;border-radius:6px;cursor:pointer";

#[derive(Clone, Copy)]
enum Job {
    Add,
    Remove,
}

#[derive(Clone)]
struct Panel {
    window: Window,
    document: Document,
    collection: u64,
    ids: HtmlTextAreaElement,
    plan: Element,
    status: Element,
    bar: Element,
    add: HtmlElement,
    remove: HtmlElement,
    subscribed: HtmlElement,
    reload: HtmlElement,
    /// Removing asks for a second click; this is set after the first.
    remove_armed: Rc<Cell<bool>>,
    /// The list came from the basket, which is emptied once it is added.
    from_basket: Rc<Cell<bool>>,
}

pub fn show(
    window: Window,
    document: Document,
    collection: u64,
    handed_over: Option<Vec<u64>>,
) -> Result<(), JsValue> {
    let basket = crate::read_basket(&window);
    let from_basket = handed_over.is_none() && !basket.is_empty();
    let ids = handed_over.unwrap_or(basket);

    let body = document.body().ok_or("no body")?;
    let root = child(&document, &body, "div", PANEL_STYLE, "")?;
    child(&document, &root, "b", "font-size:14px", "GMod Collection Builder")?;
    let source = if from_basket { "From your basket:" } else { "Addon ids or links, one per line:" };
    child(&document, &root, "div", MUTED_STYLE, source)?;
    let textarea: HtmlTextAreaElement =
        child(&document, &root, "textarea", TEXTAREA_STYLE, "")?.dyn_into()?;
    textarea.set_value(&ids.iter().map(u64::to_string).collect::<Vec<_>>().join("\n"));
    let plan = child(&document, &root, "div", "margin-bottom:8px", "")?;

    let buttons = child(&document, &root, "div", "", "")?;
    let panel = Panel {
        add: button(&document, &buttons, BUTTON_STYLE, "Add")?,
        remove: button(&document, &buttons, DANGER_BUTTON_STYLE, "Remove")?,
        subscribed: button(&document, &buttons, QUIET_BUTTON_STYLE, "Use my subscribed items")?,
        reload: button(&document, &buttons, QUIET_BUTTON_STYLE, "Reload page")?,
        bar: {
            let track = child(&document, &root, "div", BAR_STYLE, "")?;
            child(&document, &track, "div", "height:100%;width:0;background:#4a9eff", "")?
        },
        status: child(&document, &root, "div", "min-height:1.2em", "")?,
        ids: textarea,
        plan,
        window,
        document,
        collection,
        remove_armed: Rc::new(Cell::new(false)),
        from_basket: Rc::new(Cell::new(from_basket)),
    };
    panel.reload.set_hidden(true);
    panel.wire();
    panel.refresh();
    Ok(())
}

impl Panel {
    fn wire(&self) {
        let panel = self.clone();
        let on_input = Closure::<dyn FnMut()>::new(move || {
            panel.from_basket.set(false);
            panel.refresh();
        });
        self.ids.set_oninput(Some(on_input.as_ref().unchecked_ref()));
        on_input.forget();

        let panel = self.clone();
        on_click(&self.add, move || spawn_local(panel.clone().run(Job::Add)));

        let panel = self.clone();
        on_click(&self.remove, move || {
            if panel.remove_armed.replace(true) {
                spawn_local(panel.clone().run(Job::Remove));
            } else {
                let count = panel.unlisted().len();
                panel.remove.set_text_content(Some(&format!("Really remove {count}? Click again")));
            }
        });

        let panel = self.clone();
        on_click(&self.subscribed, move || {
            let subscribed = ids_on_page(&panel.document, SUBSCRIBED);
            if subscribed.is_empty() {
                panel.say("No subscribed items found on this page. Steam lists them further up, under \"Add items\".");
                return;
            }
            let lines: Vec<String> = subscribed.iter().map(u64::to_string).collect();
            panel.ids.set_value(&lines.join("\n"));
            panel.from_basket.set(false);
            panel.refresh();
            panel.say(&format!("Loaded your {} subscribed items. Copy them from the box to import them in the app.", lines.len()));
        });

        let window = self.window.clone();
        on_click(&self.reload, move || {
            let _ = window.location().reload();
        });
    }

    fn wanted(&self) -> Vec<u64> {
        handoff::parse_id_list(&self.ids.value())
    }

    fn members(&self) -> HashSet<u64> {
        ids_on_page(&self.document, MEMBERS).into_iter().collect()
    }

    fn unlisted(&self) -> Vec<u64> {
        logic::unlisted(&self.wanted(), &self.members())
    }

    /// Updates the plan line and button labels to match the list.
    fn refresh(&self) {
        let wanted = self.wanted();
        let members = self.members();
        let to_add = logic::pending(&wanted, &members).len();
        // A basket is things to add, never the whole of what the collection
        // should be, so removing is not offered for it.
        let whole_list = !wanted.is_empty() && !self.from_basket.get();
        let to_remove = if whole_list { logic::unlisted(&wanted, &members).len() } else { 0 };
        self.plan.set_text_content(Some(&logic::plan(wanted.len(), to_add, to_remove)));
        self.add.set_text_content(Some(&format!("Add {to_add}")));
        self.add.set_hidden(to_add == 0);
        self.remove.set_text_content(Some(&format!("Remove the other {to_remove}")));
        self.remove.set_hidden(to_remove == 0);
        self.remove_armed.set(false);
    }

    fn say(&self, text: &str) {
        self.status.set_text_content(Some(text));
    }

    fn set_busy(&self, busy: bool) {
        for control in [&self.add, &self.remove, &self.subscribed] {
            let _ = if busy {
                control.set_attribute("disabled", "")
            } else {
                control.remove_attribute("disabled")
            };
        }
    }

    fn set_progress(&self, done: usize, total: usize) {
        let percent = (done * 100).checked_div(total).unwrap_or(0);
        let _ = self.bar.set_attribute(
            "style",
            &format!("height:100%;width:{percent}%;background:#4a9eff;transition:width .3s"),
        );
    }

    async fn run(self, job: Job) {
        let cookies = self
            .document
            .dyn_ref::<HtmlDocument>()
            .and_then(|d| d.cookie().ok())
            .unwrap_or_default();
        let Some(session) = logic::session_id(&cookies) else {
            self.say("Could not find your Steam session. Are you logged in?");
            return;
        };
        let wanted = self.wanted();
        let (todo, url, doing, done) = match job {
            Job::Add => (logic::pending(&wanted, &self.members()), ADD_URL, "Adding", "Added"),
            Job::Remove => (self.unlisted(), REMOVE_URL, "Removing", "Removed"),
        };
        if todo.is_empty() {
            self.say("Nothing to do.");
            return;
        }
        self.set_busy(true);

        let mut succeeded = 0;
        let mut failed = Vec::new();
        let mut streak = 0;
        let mut attempted = 0;
        for (i, &id) in todo.iter().enumerate() {
            self.say(&format!("{doing} {} of {}…", i + 1, todo.len()));
            attempted = i + 1;
            let body = logic::child_body(self.collection, session, id);
            let ok = match (post_form(&self.window, url, &body).await, job) {
                (Ok(answer), Job::Add) => logic::is_success(&answer),
                (Ok(answer), Job::Remove) => logic::removal_succeeded(&answer),
                (Err(_), _) => false,
            };
            if ok {
                succeeded += 1;
                streak = 0;
            } else {
                failed.push(id);
                streak += 1;
                if streak >= MAX_FAILURE_STREAK {
                    break;
                }
            }
            self.set_progress(attempted, todo.len());
            sleep(&self.window, DELAY_MS).await;
        }

        let left = failed.len() + todo.len() - attempted;
        let mut summary = format!("{done} {succeeded}");
        if left > 0 {
            let why = if streak >= MAX_FAILURE_STREAK {
                "Stopped after repeated failures: check that you own this collection and are logged in"
            } else {
                "Some did not go through (removed or private addons cannot be added)"
            };
            summary.push_str(&format!(". {left} left. {why}"));
        }
        summary.push_str(". Reload the page to see the result.");
        if matches!(job, Job::Add) && left == 0 && self.from_basket.get() {
            crate::write_basket(&self.window, &[]);
            summary.push_str(" Your basket was emptied.");
        }
        self.set_busy(false);
        self.add.set_hidden(true);
        self.remove.set_hidden(true);
        self.reload.set_hidden(false);
        self.say(&summary);
    }
}
