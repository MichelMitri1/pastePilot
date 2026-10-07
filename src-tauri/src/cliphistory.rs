//! Local clipboard history: student messages PastePilot captured, replies it
//! produced, and (only if you turn it on) text you copy yourself.
//!
//! Privacy: stays in the local database, capped by count and age, never sent
//! anywhere unless you pick an item for a reply. Copies marked as concealed or
//! transient (password managers do this) are never recorded.

use crate::db::{self, Db};
use crate::platform::pasteboard;
use crate::state::AppState;
use rusqlite::params;
use serde::Serialize;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const MAX_ITEM_CHARS: usize = 20_000;

/// Clipboard changes PastePilot makes itself (pastes, Cmd+C capture) are not "copies".
static OWN_CHANGE: AtomicIsize = AtomicIsize::new(-1);
static SUPPRESS_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
/// changeCount the watcher last looked at, and the last change that was an image (+ when).
static LAST_SEEN: AtomicIsize = AtomicIsize::new(-1);
static IMAGE_CHANGE: AtomicIsize = AtomicIsize::new(-2);
static IMAGE_AT_MS: AtomicU64 = AtomicU64::new(0);

/// The clipboard image, if you copied it in the last `max_age` (an old screenshot is never picked up).
pub fn recent_image(max_age: Duration) -> Option<String> {
    let count = pasteboard::change_count();
    let fresh = if count == IMAGE_CHANGE.load(Ordering::SeqCst) {
        now_ms().saturating_sub(IMAGE_AT_MS.load(Ordering::SeqCst)) <= max_age.as_millis() as u64
    } else {
        // Copied within the last second, before the watcher noticed.
        count != LAST_SEEN.load(Ordering::SeqCst) && count != OWN_CHANGE.load(Ordering::SeqCst)
    };
    if fresh && pasteboard::has_image() {
        pasteboard::image_data_url()
    } else {
        None
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Call right after PastePilot writes to the clipboard.
pub fn mark_own_change() {
    OWN_CHANGE.store(pasteboard::change_count(), Ordering::SeqCst);
}

/// Ignore clipboard changes for a moment (selection capture borrows the clipboard).
pub fn suppress_for(d: Duration) {
    SUPPRESS_UNTIL_MS.store(now_ms() + d.as_millis() as u64, Ordering::SeqCst);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: i64,
    pub kind: String,
    pub content: String,
    pub source: String,
    pub created_at: i64,
}

pub fn record(app: &AppHandle, kind: &str, content: &str, source: &str) {
    let state = app.state::<AppState>();
    let settings = state.settings();
    if !settings.clipboard_history || content.trim().is_empty() || content.chars().count() > MAX_ITEM_CHARS {
        return;
    }
    let db = state.db();
    let _ = insert(&db, kind, content.trim(), source, settings.clipboard_max_items as i64, settings.clipboard_max_days as i64);
}

fn insert(db: &Db, kind: &str, content: &str, source: &str, max_items: i64, max_days: i64) -> rusqlite::Result<()> {
    let last: Option<String> = db
        .query_row("SELECT content FROM clipboard_history WHERE kind = ?1 ORDER BY id DESC LIMIT 1", params![kind], |r| r.get(0))
        .ok();
    if last.as_deref() == Some(content) {
        return Ok(());
    }
    db.execute(
        "INSERT INTO clipboard_history (kind, content, source, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![kind, content, source, db::now()],
    )?;
    prune(db, max_items, max_days)
}

fn prune(db: &Db, max_items: i64, max_days: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM clipboard_history WHERE created_at < ?1", params![db::now() - max_days.max(1) * 86_400])?;
    db.execute(
        "DELETE FROM clipboard_history WHERE id NOT IN (SELECT id FROM clipboard_history ORDER BY id DESC LIMIT ?1)",
        params![max_items.max(10)],
    )?;
    Ok(())
}

pub fn list(db: &Db, limit: usize) -> rusqlite::Result<Vec<ClipItem>> {
    let mut stmt = db.prepare("SELECT id, kind, content, source, created_at FROM clipboard_history ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok(ClipItem { id: r.get(0)?, kind: r.get(1)?, content: r.get(2)?, source: r.get(3)?, created_at: r.get(4)? })
    })?;
    rows.collect()
}

pub fn get_many(db: &Db, ids: &[i64]) -> Vec<ClipItem> {
    let mut items: Vec<ClipItem> = list(db, 1000).unwrap_or_default().into_iter().filter(|i| ids.contains(&i.id)).collect();
    items.sort_by_key(|i| i.id); // oldest first: the order they were sent
    items
}

pub fn delete(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM clipboard_history; ")
}

/// Watches the system clipboard for text you copy (only while the setting is on).
/// Event-light: one changeCount read per second.
pub fn start_watcher(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last = pasteboard::change_count();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let count = pasteboard::change_count();
            if count == last {
                continue;
            }
            last = count;
            LAST_SEEN.store(count, Ordering::SeqCst);
            // Remember when an image was copied (no content is stored), for GitHub Debug screenshots.
            if count != OWN_CHANGE.load(Ordering::SeqCst) && pasteboard::has_image() {
                IMAGE_CHANGE.store(count, Ordering::SeqCst);
                IMAGE_AT_MS.store(now_ms(), Ordering::SeqCst);
            }
            let settings = app.state::<AppState>().settings();
            if !settings.clipboard_history || !settings.clipboard_watch {
                continue;
            }
            if count == OWN_CHANGE.load(Ordering::SeqCst) || now_ms() < SUPPRESS_UNTIL_MS.load(Ordering::SeqCst) {
                continue;
            }
            if pasteboard::is_sensitive() {
                continue;
            }
            if let Some(text) = pasteboard::read_string() {
                let source = crate::platform::apps::frontmost_pid().and_then(crate::platform::apps::bundle_id).unwrap_or_default();
                record(&app, "copied", &text, &source);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_and_prunes() {
        let db = db::open_in_memory().unwrap();
        insert(&db, "student", "a", "", 10, 7).unwrap();
        insert(&db, "student", "a", "", 10, 7).unwrap();
        assert_eq!(list(&db, 50).unwrap().len(), 1);
        for i in 0..20 {
            insert(&db, "reply", &format!("r{i}"), "", 10, 7).unwrap();
        }
        assert_eq!(list(&db, 50).unwrap().len(), 10);
    }
}
