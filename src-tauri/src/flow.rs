//! The core workflow and its follow-up actions.
//!
//! Generate: selection → conversation memory → mode → knowledge + examples →
//!           OpenAI (streamed) → clipboard → paste → rewrite bar.
//! Rewrite:  last reply + its cached context → OpenAI → replace in place.
//!
//! Return/Enter is never pressed anywhere in this file.

use crate::db::{self, Db, Role};
use crate::macos::{action_bar, apps, ax, focus_tracker, hud, keys, pasteboard};
use crate::memory::{self, Scope, Turn};
use crate::modes::{self, Mode};
use crate::rewrite::{self, RewriteAction, RewriteDelivery};
use crate::settings::Settings;
use crate::state::{AppState, LastReply};
use crate::sent_watch::{self, Target};
use crate::{prompt, retrieval, selection, tray, windows};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Hotkey,
    Menu,
    /// Start a fresh conversation for this page, then generate.
    NewConversation,
}

#[derive(Clone, Copy)]
pub enum Followup {
    Rewrite(RewriteAction),
    /// Regenerate the last reply with a different mode.
    Mode(Mode),
}

enum Delivery {
    Pasted,
    /// Auto-paste is on but there was no text input to paste into.
    CopiedNoInput,
    /// Auto-paste is turned off.
    Copied,
}

static BUSY: AtomicBool = AtomicBool::new(false);

const SHORT: Duration = Duration::from_millis(1300);
const MEDIUM: Duration = Duration::from_millis(2200);
const LONG: Duration = Duration::from_millis(4000);

/// Runs `f` unless another generation/rewrite is in flight.
fn exclusive<F>(app: AppHandle, f: impl FnOnce(AppHandle) -> F + Send + 'static)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        f(app).await;
        BUSY.store(false, Ordering::SeqCst);
    });
}

/// Entry point for the hotkeys and the "Generate Reply" menu item.
pub fn trigger(app: AppHandle, how: Trigger) {
    exclusive(app, move |app| async move { generate(&app, how).await });
}

/// Rewrite bar buttons, ⌥1–⌥5, and the tray's Rewrite menu.
pub fn trigger_followup(app: AppHandle, action: Followup) {
    exclusive(app, move |app| async move { followup(&app, action).await });
}

// ---------------------------------------------------------------------------
// Generate
// ---------------------------------------------------------------------------

struct Prepared {
    conversation_id: Option<i64>,
    student_message_id: Option<i64>,
    history: Vec<Turn>,
    mode: Mode,
    context: String,
    user: String,
}

async fn generate(app: &AppHandle, how: Trigger) {
    if how == Trigger::Menu {
        // Let the menu bar menu close so keystrokes reach the app underneath.
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    action_bar::hide(app);
    sent_watch::stop();

    if !ax::is_trusted() {
        ax::prompt_for_trust();
        hud::show(app, "Allow Accessibility: System Settings → Privacy & Security → Accessibility", Some(LONG));
        windows::open_settings(app);
        return;
    }

    let state = app.state::<AppState>();
    let Some(target_pid) = apps::frontmost_pid() else {
        hud::show(app, "No active app found.", Some(MEDIUM));
        return;
    };
    let settings = state.settings();

    // 1. Read the selection (AX first, Cmd+C fallback) and where it came from.
    let (memory_on, auto_detect) = (settings.memory_enabled, settings.memory_auto_detect);
    let captured = tauri::async_runtime::spawn_blocking(move || {
        let text = selection::capture(target_pid)?;
        let scope = memory_on.then(|| memory::detect_scope(target_pid, auto_detect));
        Some((text, scope))
    })
    .await
    .ok()
    .flatten();
    let Some((selected, scope)) = captured else {
        hud::show(app, "No text selected.", Some(SHORT));
        return;
    };

    let Some(api_key) = state.api_key() else {
        hud::show(app, "Add your OpenAI API key in Settings.", Some(LONG));
        windows::open_settings(app);
        return;
    };

    // 2. Local context: memory, mode, knowledge, examples (all sub-millisecond SQLite).
    let prepared = {
        let db = state.db();
        prepare(&db, &settings, &selected, scope.as_ref(), how == Trigger::NewConversation)
    };
    if prepared.conversation_id.is_some() {
        state.set_current_conversation(prepared.conversation_id);
    }
    if settings.show_hud {
        hud::show(app, &format!("Generating · {}", prepared.mode.label()), None);
    }

    // 3. Ask OpenAI. On failure, the clipboard is left untouched.
    let system = state.system_prompt();
    let messages = prompt::messages(&system, &prepared.context, &prepared.history, &prepared.user, &[]);
    let reply = match state.openai.generate(&api_key, &settings.model, &messages).await {
        Ok(text) if !text.trim().is_empty() => prompt::clean_reply(&text),
        Ok(_) => {
            hud::show(app, "OpenAI returned an empty reply.", Some(LONG));
            return;
        }
        Err(message) => {
            hud::show(app, &message, Some(LONG));
            return;
        }
    };

    let agent_message_id =
        prepared.conversation_id.and_then(|c| memory::record_agent(&state.db(), c, &reply).ok());
    let scope_key = scope.as_ref().map(|s| s.key.clone());
    state.set_last_reply(Some(LastReply {
        target_pid,
        scope_key: scope_key.clone(),
        student_message_id: prepared.student_message_id,
        agent_message_id,
        student_text: selected,
        user: prepared.user,
        history: prepared.history,
        context: prepared.context,
        mode: prepared.mode,
        reply: reply.clone(),
    }));

    // 4. Clipboard + paste.
    let auto_paste = settings.auto_paste;
    let (delivery, input) = tauri::async_runtime::spawn_blocking(move || deliver(target_pid, &reply, auto_paste))
        .await
        .unwrap_or((Delivery::CopiedNoInput, None));
    watch_for_send(app, target_pid, input, agent_message_id, scope_key, &settings);

    let mode = prepared.mode.label();
    match delivery {
        Delivery::Pasted if settings.show_hud => hud::show(app, &format!("Reply pasted · {mode}"), Some(SHORT)),
        Delivery::Pasted => hud::hide(app),
        Delivery::CopiedNoInput => hud::show(app, "Reply copied to clipboard.", Some(MEDIUM)),
        Delivery::Copied => hud::show(app, &format!("Reply copied · {mode}"), Some(SHORT)),
    }
    if settings.rewrite_bar {
        action_bar::show(app, prepared.mode);
    }
}

fn prepare(db: &Db, settings: &Settings, text: &str, scope: Option<&Scope>, fresh: bool) -> Prepared {
    let mut conversation_id = None;
    let mut student_message_id = None;
    let mut history = Vec::new();
    let mut previous_mode = None;

    if let Some(scope) = scope {
        if fresh {
            let _ = db::close_scope(db, &scope.key);
        }
        if let Ok(rec) = memory::record_student(db, scope, text, settings) {
            conversation_id = Some(rec.conversation_id);
            student_message_id = Some(rec.student_message_id);
            history = rec.history;
            previous_mode = rec.previous_mode;
        }
    }

    let mode = Mode::from_id(&settings.mode_override).unwrap_or_else(|| modes::classify(text, previous_mode));
    if let Some(id) = student_message_id {
        let _ = db::set_message_mode(db, id, mode.id());
    }
    let context = build_context(db, settings, text, &history, mode);
    Prepared { conversation_id, student_message_id, history, mode, context, user: prompt::user_message(text) }
}

/// Mode instructions + the knowledge entries and examples relevant to this message.
fn build_context(db: &Db, settings: &Settings, text: &str, history: &[Turn], mode: Mode) -> String {
    // Short follow-ups ("I sent it above") borrow search terms from the previous student turn.
    let mut query = text.to_string();
    if let Some(prev) = history.iter().rev().find(|t| t.role == Role::Student) {
        query.push('\n');
        query.push_str(&prev.content);
    }
    let knowledge = if settings.kb_enabled {
        retrieval::knowledge(db, &query, mode, settings.max_kb_entries as usize)
    } else {
        Vec::new()
    };
    let examples = if settings.examples_enabled {
        retrieval::examples(db, &query, mode, settings.max_examples as usize)
    } else {
        Vec::new()
    };
    prompt::build_context(settings, mode, &knowledge, &examples, !history.is_empty())
}

/// Watches the reply box so the version you actually send (with your edits)
/// replaces the generated one in conversation memory.
fn watch_for_send(
    app: &AppHandle,
    pid: i32,
    input: Option<ax::Element>,
    agent_message_id: Option<i64>,
    scope_key: Option<String>,
    settings: &Settings,
) {
    if let (Some(input), Some(agent_message_id)) = (input, agent_message_id) {
        sent_watch::start(app, Target { pid, input, agent_message_id, scope_key, auto_detect: settings.memory_auto_detect });
    }
}

/// Puts the reply on the clipboard and, if possible, pastes it into the text
/// input the user was using. Never presses Return.
/// Also returns the text input the reply went into (or will likely be pasted into).
fn deliver(pid: i32, reply: &str, auto_paste: bool) -> (Delivery, Option<ax::Element>) {
    pasteboard::write_string(reply);
    if !auto_paste {
        return (Delivery::Copied, focus_tracker::remembered_input(pid));
    }
    if !apps::ensure_frontmost(pid) {
        return (Delivery::CopiedNoInput, None);
    }

    // a) Focus is already in a text input (e.g. you clicked the reply box while it was generating).
    if let Some(input) = focused_input(pid) {
        keys::paste();
        return (Delivery::Pasted, Some(input));
    }

    // b) Put focus back into the last text input you used in that app.
    if let Some(input) = focus_tracker::remembered_input(pid) {
        if ax::focus(&input) && wait_for_focused_input(pid, Duration::from_millis(250)) {
            keys::paste();
            return (Delivery::Pasted, Some(input));
        }
    }

    (Delivery::CopiedNoInput, None)
}

fn focused_input(pid: i32) -> Option<ax::Element> {
    ax::focused_element(pid).filter(ax::is_editable)
}

fn wait_for_focused_input(pid: i32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if focused_input(pid).is_some() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(15));
    }
}

// ---------------------------------------------------------------------------
// Rewrites
// ---------------------------------------------------------------------------

async fn followup(app: &AppHandle, action: Followup) {
    let state = app.state::<AppState>();
    let Some(mut last) = state.last_reply() else {
        hud::show(app, "No reply to rewrite yet.", Some(MEDIUM));
        return;
    };
    let Some(api_key) = state.api_key() else {
        hud::show(app, "Add your OpenAI API key in Settings.", Some(LONG));
        return;
    };
    let settings = state.settings();
    let system = state.system_prompt();

    let (label, messages, mode, context) = match action {
        Followup::Rewrite(a) => {
            // Reuses the exact context of the original request: no retrieval, no rebuild.
            let tail = [(Role::Agent, last.reply.as_str()), (Role::Student, prompt::rewrite_instruction(a))];
            let m = prompt::messages(&system, &last.context, &last.history, &last.user, &tail);
            (a.label().to_string(), m, last.mode, last.context.clone())
        }
        Followup::Mode(mode) => {
            let context = build_context(&state.db(), &settings, &last.student_text, &last.history, mode);
            let m = prompt::messages(&system, &context, &last.history, &last.user, &[]);
            (mode.label().to_string(), m, mode, context)
        }
    };

    if settings.show_hud {
        hud::show(app, &format!("Rewriting · {label}"), None);
    }
    let new_reply = match state.openai.generate(&api_key, &settings.model, &messages).await {
        Ok(text) if !text.trim().is_empty() => prompt::clean_reply(&text),
        Ok(_) => {
            hud::show(app, "OpenAI returned an empty reply.", Some(LONG));
            return;
        }
        Err(message) => {
            hud::show(app, &message, Some(LONG));
            return;
        }
    };

    {
        let db = state.db();
        if let Some(id) = last.agent_message_id {
            let _ = db::update_message(&db, id, &new_reply);
        }
        if let (Followup::Mode(m), Some(id)) = (action, last.student_message_id) {
            let _ = db::set_message_mode(&db, id, m.id());
        }
    }
    let old_reply = std::mem::replace(&mut last.reply, new_reply.clone());
    last.mode = mode;
    last.context = context;
    let pid = last.target_pid;
    let agent_message_id = last.agent_message_id;
    let scope_key = last.scope_key.clone();
    state.set_last_reply(Some(last));
    sent_watch::stop();

    let auto_paste = settings.auto_paste;
    let (delivery, input) =
        tauri::async_runtime::spawn_blocking(move || rewrite::deliver(pid, &old_reply, &new_reply, auto_paste))
            .await
            .unwrap_or((RewriteDelivery::CopiedNotFound, None));
    watch_for_send(app, pid, input, agent_message_id, scope_key, &settings);
    match delivery {
        RewriteDelivery::Replaced if settings.show_hud => hud::show(app, &format!("Reply updated · {label}"), Some(SHORT)),
        RewriteDelivery::Replaced => hud::hide(app),
        RewriteDelivery::Copied => hud::show(app, "New version copied", Some(SHORT)),
        RewriteDelivery::CopiedNotFound => hud::show(app, "New version copied. Select your draft and press ⌘V.", Some(LONG)),
    }
    if settings.rewrite_bar {
        action_bar::show(app, mode);
    }
}

// ---------------------------------------------------------------------------
// Small actions (menu bar, rewrite bar)
// ---------------------------------------------------------------------------

/// Saves the last exchange as a style example. Uses the reply box's current
/// text when readable, so your manual edits are what gets learned.
pub fn save_last_as_example(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(last) = state.last_reply() else {
        hud::show(app, "No reply to save yet.", Some(MEDIUM));
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let reply = match rewrite::current_input_text(last.target_pid) {
            // Keep only the reply if the box also has other text in it.
            Some(text) if rewrite::contains_text(&text, &last.reply) => last.reply.clone(),
            Some(text) if text.chars().count() < 4000 => text.trim().to_string(),
            _ => last.reply.clone(),
        };
        let example = db::ReplyExample {
            id: None,
            student_message: last.student_text.trim().to_string(),
            reply,
            category: last.mode.id().to_string(),
        };
        let saved = db::save_example(&state.db(), &example).is_ok();
        if saved {
            state.refresh_style_profile();
            hud::show(&app, "Saved as reply example", Some(SHORT));
        } else {
            hud::show(&app, "Couldn't save the example.", Some(MEDIUM));
        }
    });
}

pub fn new_conversation(app: &AppHandle) {
    sent_watch::stop();
    let state = app.state::<AppState>();
    if let Some(id) = state.current_conversation() {
        let _ = db::close_conversation(&state.db(), id);
    }
    state.set_current_conversation(None);
    state.set_last_reply(None);
    action_bar::hide(app);
    hud::show(app, "New conversation started", Some(SHORT));
}

pub fn clear_current_conversation(app: &AppHandle) {
    sent_watch::stop();
    let state = app.state::<AppState>();
    if let Some(id) = state.current_conversation() {
        let _ = db::delete_conversation(&state.db(), id);
    }
    state.set_current_conversation(None);
    state.set_last_reply(None);
    action_bar::hide(app);
    hud::show(app, "Conversation context cleared", Some(SHORT));
}

/// Sticky mode override from the menu bar ("auto" or a mode id).
pub fn set_mode_override(app: &AppHandle, mode_id: &str) {
    let state = app.state::<AppState>();
    let mut settings = state.settings();
    settings.mode_override = mode_id.to_string();
    if state.replace_settings(settings.clone()).is_ok() {
        tray::sync(app, &settings);
        tray::emit_settings(app, &settings);
    }
}
