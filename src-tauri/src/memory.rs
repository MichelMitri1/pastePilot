//! Conversation memory.
//!
//! Every captured student message and every generated reply is stored
//! locally, grouped into conversations. A conversation is scoped to where the
//! message was selected: app + page URL (or the window/tab title when there is
//! no URL). Different tickets or chats therefore never share context. With automatic detection turned
//! off, everything goes into one manual session until "New Conversation".
//!
//! Only the most recent messages that fit the configured budget are sent.

use crate::db::{self, Db, Role};
use crate::macos::{apps, ax};
use crate::modes::Mode;
use crate::settings::Settings;

#[derive(Debug, Clone)]
pub struct Scope {
    pub key: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub role: Role,
    pub content: String,
}

pub struct Recorded {
    pub conversation_id: i64,
    pub student_message_id: i64,
    /// Chronological, excluding the current message.
    pub history: Vec<Turn>,
    pub previous_mode: Option<Mode>,
}

pub const MANUAL_SCOPE: &str = "manual";

/// Where the message was selected. Runs a handful of Accessibility calls (a few ms).
pub fn detect_scope(pid: i32, auto_detect: bool) -> Scope {
    if !auto_detect {
        return Scope { key: MANUAL_SCOPE.into(), title: "Manual session".into() };
    }
    let bundle = apps::bundle_id(pid).unwrap_or_else(|| format!("pid:{pid}"));
    let title = ax::window_title(pid).map(|t| normalize_title(&t)).unwrap_or_default();
    let url = ax::page_url(pid).map(|u| normalize_url(&u)).unwrap_or_default();
    let display = if !title.is_empty() { title.clone() } else if !url.is_empty() { url.clone() } else { bundle.clone() };
    // A page URL (which carries ticket/channel IDs) identifies the conversation on its own.
    // Titles can change after sending ("Replied", "Solved"), so they're only used without a URL.
    let key = if url.is_empty() { format!("{bundle}||{title}") } else { format!("{bundle}|{url}|") };
    Scope { key, title: display }
}

/// "(3) Inbox • Ticket 123" → "Inbox • Ticket 123": unread counters change while the chat doesn't.
fn normalize_title(title: &str) -> String {
    let mut t = title.trim();
    loop {
        let before = t;
        if let Some(rest) = t.strip_prefix('(') {
            if let Some(end) = rest.find(')') {
                if rest[..end].chars().all(|c| c.is_ascii_digit() || c == '+') {
                    t = rest[end + 1..].trim_start();
                }
            }
        }
        t = t.trim_start_matches(['•', '*', '●']).trim_start();
        if t == before {
            break;
        }
    }
    t.to_string()
}

fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn same_text(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

/// Stores the student message in the right conversation and loads the context before it.
pub fn record_student(db: &Db, scope: &Scope, text: &str, settings: &Settings) -> rusqlite::Result<Recorded> {
    let since = db::now() - i64::from(settings.memory_expire_hours.max(1)) * 3600;
    let conversation_id = match db::find_active_conversation(db, &scope.key, since)? {
        Some(id) => id,
        None => db::create_conversation(db, &scope.key, &scope.title)?,
    };

    // Re-selecting the same message (to regenerate) must not duplicate it.
    let last = db::last_messages(db, conversation_id, 2)?;
    let student_message_id = match (last.first(), last.get(1)) {
        (Some(m0), _) if m0.role == Role::Student && same_text(&m0.content, text) => m0.id,
        (Some(m0), Some(m1)) if m0.role == Role::Agent && m1.role == Role::Student && same_text(&m1.content, text) => {
            db::delete_message(db, m0.id)?;
            m1.id
        }
        _ => db::insert_message(db, conversation_id, Role::Student, text, None)?,
    };
    db::touch_conversation(db, conversation_id)?;

    let history = history(
        db,
        conversation_id,
        student_message_id,
        settings.memory_max_messages as usize,
        settings.memory_max_chars as usize,
    )?;
    let previous_mode = db::last_mode(db, conversation_id, student_message_id)?.and_then(|m| Mode::from_id(&m));
    Ok(Recorded { conversation_id, student_message_id, history, previous_mode })
}

/// Most recent messages before `before_id` that fit the budget, oldest first.
fn history(db: &Db, conversation_id: i64, before_id: i64, max_messages: usize, max_chars: usize) -> rusqlite::Result<Vec<Turn>> {
    if max_messages == 0 || max_chars == 0 {
        return Ok(Vec::new());
    }
    let mut turns = Vec::new();
    let mut used = 0;
    for m in db::messages_before(db, conversation_id, before_id, max_messages)? {
        let len = m.content.chars().count();
        if used + len > max_chars {
            let room = max_chars.saturating_sub(used);
            if turns.is_empty() && room > 200 {
                // Keep the end of an oversized latest message rather than nothing.
                let tail: String = m.content.chars().skip(len - room).collect();
                turns.push(Turn { role: m.role, content: format!("…{tail}") });
            }
            break;
        }
        used += len;
        turns.push(Turn { role: m.role, content: m.content });
    }
    turns.reverse();
    Ok(turns)
}

pub fn record_agent(db: &Db, conversation_id: i64, reply: &str) -> rusqlite::Result<i64> {
    db::insert_message(db, conversation_id, Role::Agent, reply, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn follow_up_sees_previous_turns() {
        let db = db::open_in_memory().unwrap();
        let scope = Scope { key: "chrome|https://support.example/t/1|Ticket 1".into(), title: "Ticket 1".into() };
        let first = record_student(&db, &scope, "My npm install isn't working.", &settings()).unwrap();
        assert!(first.history.is_empty());
        db::set_message_mode(&db, first.student_message_id, "technical").unwrap();
        record_agent(&db, first.conversation_id, "Can you send me the error you're getting?").unwrap();

        let second = record_student(&db, &scope, "I sent it above.", &settings()).unwrap();
        assert_eq!(second.conversation_id, first.conversation_id);
        assert_eq!(second.history.len(), 2);
        assert_eq!(second.history[0].content, "My npm install isn't working.");
        assert_eq!(second.history[1].role, Role::Agent);
        assert_eq!(second.previous_mode, Some(Mode::Technical));
    }

    #[test]
    fn different_scopes_never_mix() {
        let db = db::open_in_memory().unwrap();
        let a = Scope { key: "chrome|https://x/t/1|A".into(), title: "A".into() };
        let b = Scope { key: "chrome|https://x/t/2|B".into(), title: "B".into() };
        record_student(&db, &a, "Secret context for A", &settings()).unwrap();
        let rb = record_student(&db, &b, "Hello from B", &settings()).unwrap();
        assert!(rb.history.is_empty());
    }

    #[test]
    fn reselecting_same_message_regenerates_without_duplicates() {
        let db = db::open_in_memory().unwrap();
        let s = Scope { key: "k".into(), title: "t".into() };
        let r1 = record_student(&db, &s, "Where is my certificate?", &settings()).unwrap();
        record_agent(&db, r1.conversation_id, "draft 1").unwrap();
        let r2 = record_student(&db, &s, "Where is my  certificate?", &settings()).unwrap();
        assert_eq!(r1.student_message_id, r2.student_message_id);
        assert_eq!(db::conversation_counts(&db).unwrap(), (1, 1)); // old draft removed
    }

    #[test]
    fn budget_limits_history() {
        let db = db::open_in_memory().unwrap();
        let s = Scope { key: "k".into(), title: "t".into() };
        let mut st = settings();
        st.memory_max_messages = 2;
        for i in 0..5 {
            let r = record_student(&db, &s, &format!("question {i}"), &st).unwrap();
            record_agent(&db, r.conversation_id, &format!("answer {i}")).unwrap();
        }
        let r = record_student(&db, &s, "latest", &st).unwrap();
        assert_eq!(r.history.iter().map(|t| t.content.as_str()).collect::<Vec<_>>(), vec!["question 4", "answer 4"]);
    }

    #[test]
    fn closed_conversations_start_fresh() {
        let db = db::open_in_memory().unwrap();
        let s = Scope { key: "k".into(), title: "t".into() };
        let r1 = record_student(&db, &s, "old student", &settings()).unwrap();
        db::close_conversation(&db, r1.conversation_id).unwrap();
        let r2 = record_student(&db, &s, "new student", &settings()).unwrap();
        assert_ne!(r1.conversation_id, r2.conversation_id);
        assert!(r2.history.is_empty());
    }

    #[test]
    fn strips_unread_counters_from_titles() {
        assert_eq!(normalize_title("(3) Inbox • Ticket 123"), "Inbox • Ticket 123");
        assert_eq!(normalize_title("• (12) Chat"), "Chat");
        assert_eq!(normalize_title("Python (beginner)"), "Python (beginner)");
    }
}
