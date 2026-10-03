//! Multi-message cases: collect several selected (or history) messages, then
//! the next ⌥R or GitHub Debug treats them as one support case.
//!
//! ⌥A adds the current selection. A case belongs to the ticket/chat it was
//! collected in and is never used for a different one.

use crate::macos::{apps, ax, hud};
use crate::memory;
use crate::selection;
use crate::state::AppState;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const MAX_MESSAGES: usize = 12;

#[derive(Default)]
struct Case {
    messages: Vec<String>,
    scope_key: Option<String>,
}

static CASE: LazyLock<Mutex<Case>> = LazyLock::new(|| Mutex::new(Case::default()));

fn case() -> MutexGuard<'static, Case> {
    CASE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn len() -> usize {
    case().messages.len()
}

/// Adds text to the case; a different ticket starts a new case.
fn push(app: &AppHandle, text: String, scope_key: Option<String>) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let n = {
        let mut c = case();
        if c.scope_key.is_some() && scope_key.is_some() && c.scope_key != scope_key {
            c.messages.clear();
        }
        if c.scope_key.is_none() {
            c.scope_key = scope_key;
        }
        if !c.messages.contains(&text) && c.messages.len() < MAX_MESSAGES {
            c.messages.push(text);
        }
        c.messages.len()
    };
    hud::show(
        app,
        &format!("Added to case ({n} message{}). ⌥R to reply, ⌥G to debug.", if n == 1 { "" } else { "s" }),
        Some(Duration::from_millis(2200)),
    );
}

/// ⌥A / menu bar: add the current selection.
pub fn add_selection(app: AppHandle, from_menu: bool) {
    tauri::async_runtime::spawn(async move {
        if from_menu {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        if !ax::is_trusted() {
            hud::show(&app, "Allow Accessibility first (Settings → General).", Some(Duration::from_secs(3)));
            return;
        }
        let Some(pid) = apps::frontmost_pid() else { return };
        let settings = app.state::<AppState>().settings();
        let (memory_on, auto) = (settings.memory_enabled, settings.memory_auto_detect);
        let captured = tauri::async_runtime::spawn_blocking(move || {
            let text = selection::capture(pid)?;
            let scope = memory_on.then(|| memory::detect_scope(pid, auto).key);
            Some((text, scope))
        })
        .await
        .ok()
        .flatten();
        match captured {
            Some((text, scope)) => {
                crate::cliphistory::record(&app, "student", &text, "");
                push(&app, text, scope);
            }
            None => hud::show(&app, "No text selected.", Some(Duration::from_millis(1300))),
        }
    });
}

/// From Clipboard History: add picked items (oldest first).
pub fn add_texts(app: &AppHandle, texts: Vec<String>) {
    for t in texts {
        let mut c = case();
        if !c.messages.contains(&t) && c.messages.len() < MAX_MESSAGES {
            c.messages.push(t.trim().to_string());
        }
    }
    let n = len();
    hud::show(app, &format!("Case has {n} message{}. ⌥R to reply, ⌥G to debug.", if n == 1 { "" } else { "s" }), Some(Duration::from_millis(2200)));
}

pub fn clear(app: &AppHandle) {
    *case() = Case::default();
    hud::show(app, "Case cleared", Some(Duration::from_millis(1200)));
}

/// Takes the collected messages for this ticket (clearing the case), combined with
/// the current selection. Returns None when there's no case for this ticket.
pub fn take_combined(scope_key: Option<&str>, current: Option<&str>) -> Option<String> {
    let mut c = case();
    if c.messages.is_empty() {
        return None;
    }
    if let (Some(a), Some(b)) = (c.scope_key.as_deref(), scope_key) {
        if a != b {
            return None; // collected on a different ticket: never mix
        }
    }
    let mut messages = std::mem::take(&mut c.messages);
    c.scope_key = None;
    if let Some(cur) = current.map(str::trim).filter(|s| !s.is_empty()) {
        if !messages.iter().any(|m| m == cur || m.contains(cur)) {
            messages.push(cur.to_string());
        }
    }
    Some(combine(&messages))
}

pub fn combine(messages: &[String]) -> String {
    messages.iter().map(|m| m.trim()).filter(|m| !m.is_empty()).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_and_respects_scope() {
        {
            let mut c = case();
            c.messages = vec!["My npm install fails".into(), "Here's the error: EACCES".into()];
            c.scope_key = Some("ticket-1".into());
        }
        assert_eq!(take_combined(Some("ticket-2"), None), None);
        let combined = take_combined(Some("ticket-1"), Some("Any ideas?")).unwrap();
        assert_eq!(combined, "My npm install fails\n\nHere's the error: EACCES\n\nAny ideas?");
        assert_eq!(len(), 0);
    }
}
