//! Captures the reply you actually sent, including your edits.
//!
//! After a reply is pasted, the reply box is checked a few times a second
//! through Accessibility (only while a draft is pending, never otherwise).
//! When the box goes from having text to empty while you're still on the same
//! ticket, that's a send: the last text seen replaces the generated reply in
//! conversation memory. No keystrokes are recorded.
//!
//! If you switch tickets, generate a new reply, or leave the draft alone for
//! a long time, watching stops without saving anything.

use crate::db;
use crate::{analytics, feedback};
use crate::macos::{action_bar, ax};
use crate::memory;
use crate::state::AppState;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const POLL: Duration = Duration::from_millis(300);
/// Give up if the draft hasn't changed for this long.
const IDLE_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// Ignore tiny leftovers (a stray character) when deciding the box was "sent".
const MIN_SENT_CHARS: usize = 2;

static GENERATION: AtomicU64 = AtomicU64::new(0);

pub struct Target {
    pub pid: i32,
    pub input: ax::Element,
    pub agent_message_id: i64,
    /// Conversation scope when the reply was generated; None = don't check.
    pub scope_key: Option<String>,
    pub auto_detect: bool,
}

/// Starts watching (replacing any previous watch).
pub fn start(app: &AppHandle, target: Target) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || run(app, target, generation));
}

/// Stops any watch without saving.
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn run(app: AppHandle, target: Target, generation: u64) {
    let mut tracker = Tracker::new(Instant::now());
    loop {
        std::thread::sleep(POLL);
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let value = ax::value(&target.input)
            // Empty rich editors sometimes drop AXValue entirely while the element still exists.
            .or_else(|| target.input.attr_string("AXRole").map(|_| String::new()))
            .or_else(|| fallback_value(&target));
        match tracker.observe(value.as_deref(), Instant::now()) {
            Step::Continue => {}
            Step::Stop => return,
            Step::Sent(text) => {
                // A cleared box on a different ticket is a navigation, not a send.
                if same_scope(&target) && GENERATION.load(Ordering::SeqCst) == generation {
                    save(&app, target.agent_message_id, &text);
                }
                return;
            }
        }
    }
}

/// Some editors rebuild the input after sending. Look at whatever text input now has focus.
fn fallback_value(target: &Target) -> Option<String> {
    let el = ax::focused_element(target.pid)?;
    if !ax::is_editable(&el) {
        return None;
    }
    Some(ax::value(&el).unwrap_or_default())
}

fn same_scope(target: &Target) -> bool {
    match &target.scope_key {
        Some(key) => memory::detect_scope(target.pid, target.auto_detect).key == *key,
        None => true,
    }
}

fn save(app: &AppHandle, agent_message_id: i64, text: &str) {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let last = state.last_reply().filter(|l| l.agent_message_id == Some(agent_message_id));
    {
        let db = state.db();
        let _ = db::update_message(&db, agent_message_id, text);
    }
    let Some(last) = last else { return };

    // Feedback learning: how much did you change the draft before sending?
    let similarity = feedback::similarity(&last.reply, text);
    let mode = last.mode.label();
    let mut examples_changed = false;
    {
        let db = state.db();
        analytics::log(&db, "sent", Some(mode), None, Some(similarity));
        if settings.feedback_learning && similarity < feedback::SIGNIFICANT {
            feedback::record(&db, Some(last.mode.id()), &last.student_text, &last.reply, text, similarity);
            // A heavily rewritten reply is a great example of your style.
            if settings.feedback_save_examples && similarity < 0.75 && text.chars().count() < 4000 {
                let example = db::ReplyExample {
                    id: None,
                    student_message: last.student_text.trim().to_string(),
                    reply: text.trim().to_string(),
                    category: last.mode.id().to_string(),
                };
                if db::save_example(&db, &example).is_ok() {
                    analytics::log(&db, "example", Some(mode), Some("learned"), None);
                    examples_changed = true;
                }
            }
        }
    } // database lock released before the profiles are rebuilt
    if settings.feedback_learning && similarity < feedback::SIGNIFICANT {
        state.refresh_edit_profile();
        if examples_changed {
            state.refresh_style_profile();
        }
    }

    // The draft is gone from the box, so rewriting it no longer makes sense.
    state.set_last_reply(None);
    action_bar::hide(app);
}

#[derive(Debug, PartialEq)]
enum Step {
    Continue,
    Stop,
    Sent(String),
}

/// Pure decision logic, separated so it can be unit tested.
struct Tracker {
    last_text: Option<String>,
    last_change: Instant,
}

impl Tracker {
    fn new(now: Instant) -> Self {
        Self { last_text: None, last_change: now }
    }

    /// `value`: the box's current text, or None if it can't be read anymore.
    fn observe(&mut self, value: Option<&str>, now: Instant) -> Step {
        let Some(value) = value else { return Step::Stop };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return match self.last_text.take() {
                Some(text) if text.chars().count() >= MIN_SENT_CHARS => Step::Sent(text),
                // Never saw the reply in the box (e.g. read before the paste landed): keep waiting.
                _ if now.duration_since(self.last_change) > IDLE_TIMEOUT => Step::Stop,
                _ => Step::Continue,
            };
        }
        if self.last_text.as_deref() != Some(trimmed) {
            self.last_text = Some(trimmed.to_string());
            self.last_change = now;
        } else if now.duration_since(self.last_change) > IDLE_TIMEOUT {
            return Step::Stop;
        }
        Step::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_edited_text_when_box_clears() {
        let t0 = Instant::now();
        let mut t = Tracker::new(t0);
        assert_eq!(t.observe(Some("Hey! Try deleting node_modules."), t0), Step::Continue);
        assert_eq!(t.observe(Some("Hey Sam! Try deleting node_modules :)"), t0), Step::Continue);
        assert_eq!(t.observe(Some("\n"), t0), Step::Sent("Hey Sam! Try deleting node_modules :)".into()));
    }

    #[test]
    fn waits_until_text_has_been_seen() {
        let t0 = Instant::now();
        let mut t = Tracker::new(t0);
        assert_eq!(t.observe(Some(""), t0), Step::Continue);
        assert_eq!(t.observe(Some("reply"), t0), Step::Continue);
        assert_eq!(t.observe(Some(""), t0), Step::Sent("reply".into()));
    }

    #[test]
    fn stops_when_input_disappears_or_idles() {
        let t0 = Instant::now();
        let mut t = Tracker::new(t0);
        assert_eq!(t.observe(Some("draft"), t0), Step::Continue);
        assert_eq!(t.observe(None, t0), Step::Stop);

        let mut t = Tracker::new(t0);
        t.observe(Some("draft"), t0);
        assert_eq!(t.observe(Some("draft"), t0 + IDLE_TIMEOUT + Duration::from_secs(1)), Step::Stop);
    }
}
