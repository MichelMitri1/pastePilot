//! Local usage metrics for the dashboard. Counts only: what happened and
//! when, never message text.

use crate::db::{self, Db};
use rusqlite::params;
use serde::Serialize;

pub fn log(db: &Db, kind: &str, mode: Option<&str>, detail: Option<&str>, value: Option<f64>) {
    let _ = db.execute(
        "INSERT INTO events (kind, mode, detail, value, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![kind, mode, detail, value, db::now()],
    );
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub label: String,
    pub count: i64,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// Days before today (0 = today).
    pub days_ago: i64,
    pub replies: i64,
    pub debug: i64,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub replies: i64,
    pub debug_cases: i64,
    pub rewrites: i64,
    pub sent: i64,
    pub edited: i64,
    pub examples_learned: i64,
    pub minutes_saved: f64,
    pub modes: Vec<Count>,
    pub issue_types: Vec<Count>,
    pub project_types: Vec<Count>,
    pub rewrite_actions: Vec<Count>,
    pub days: Vec<Day>,
}

fn count(db: &Db, kind: &str, since: i64) -> i64 {
    db.query_row("SELECT COUNT(*) FROM events WHERE kind = ?1 AND created_at >= ?2", params![kind, since], |r| r.get(0)).unwrap_or(0)
}

fn group(db: &Db, kind: &str, column: &str, since: i64) -> Vec<Count> {
    let sql = format!(
        "SELECT {column}, COUNT(*) AS n FROM events WHERE kind = ?1 AND created_at >= ?2 AND {column} IS NOT NULL AND {column} != ''
         GROUP BY {column} ORDER BY n DESC LIMIT 8"
    );
    let Ok(mut stmt) = db.prepare(&sql) else { return Vec::new() };
    stmt.query_map(params![kind, since], |r| Ok(Count { label: r.get(0)?, count: r.get(1)? }))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// `range_days` = 0 means all time. Minutes saved uses your per-task estimates.
pub fn dashboard(db: &Db, range_days: i64, minutes_per_reply: f64, minutes_per_debug: f64) -> Dashboard {
    let now = db::now();
    let since = if range_days > 0 { now - range_days * 86_400 } else { 0 };
    let replies = count(db, "reply", since);
    let debug_cases = count(db, "debug", since);
    let edited: i64 = db
        .query_row("SELECT COUNT(*) FROM events WHERE kind = 'sent' AND created_at >= ?1 AND value < 0.85", params![since], |r| r.get(0))
        .unwrap_or(0);

    let span = if range_days > 0 { range_days.min(90) } else { 30 };
    let start_of_today = now - now.rem_euclid(86_400);
    let mut days: Vec<Day> = (0..span).rev().map(|d| Day { days_ago: d, ..Default::default() }).collect();
    if let Ok(mut stmt) = db.prepare("SELECT kind, created_at FROM events WHERE kind IN ('reply', 'debug') AND created_at >= ?1") {
        if let Ok(rows) = stmt.query_map(params![start_of_today - (span - 1) * 86_400], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
            for (kind, at) in rows.flatten() {
                let ago = if at >= start_of_today { 0 } else { (start_of_today - at - 1) / 86_400 + 1 };
                if let Some(day) = days.iter_mut().find(|d| d.days_ago == ago) {
                    if kind == "reply" { day.replies += 1 } else { day.debug += 1 }
                }
            }
        }
    }

    Dashboard {
        replies,
        debug_cases,
        rewrites: count(db, "rewrite", since),
        sent: count(db, "sent", since),
        edited,
        examples_learned: count(db, "example", since),
        minutes_saved: replies as f64 * minutes_per_reply + debug_cases as f64 * minutes_per_debug,
        modes: group(db, "reply", "mode", since),
        issue_types: group(db, "debug", "detail", since),
        project_types: group(db, "debug", "mode", since),
        rewrite_actions: group(db, "rewrite", "detail", since),
        days,
    }
}

pub fn reset(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM events;")
}

/// Buckets a diagnosis into a broad issue type for the dashboard.
pub fn issue_type(check_kinds: &[String], text: &str) -> &'static str {
    let t = text.to_lowercase();
    let has = |k: &str| check_kinds.iter().any(|c| c == k);
    if has("casing") || t.contains("casing") || t.contains("case-sensitive") {
        "Import casing"
    } else if has("import") || t.contains("import path") || t.contains("module not found") || t.contains("can't resolve") {
        "Imports & paths"
    } else if has("export") || t.contains("export") {
        "Exports"
    } else if has("dependency") || t.contains("package.json") || t.contains("npm install") || t.contains("dependency") {
        "Dependencies"
    } else if has("jsx") || t.contains("usestate") || t.contains("useeffect") || t.contains("props") || t.contains("jsx") {
        "React / JSX"
    } else if has("html") || t.contains("closing tag") || t.contains("html") {
        "HTML structure"
    } else if has("css") || ["css", "media query", "display", "flex", "grid", "selector", "specificity", "style"].iter().any(|k| t.contains(k)) {
        "CSS & layout"
    } else if t.contains("route") || t.contains("router") {
        "Routing"
    } else if ["deploy", "build", "vite", "netlify", "vercel"].iter().any(|k| t.contains(k)) {
        "Build & deploy"
    } else {
        "Other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_counts() {
        let db = db::open_in_memory().unwrap();
        log(&db, "reply", Some("Technical"), None, None);
        log(&db, "reply", Some("Billing"), None, None);
        log(&db, "debug", Some("React · Vite"), Some("CSS & layout"), None);
        log(&db, "sent", None, None, Some(0.5));
        log(&db, "sent", None, None, Some(0.97));
        let d = dashboard(&db, 7, 2.0, 10.0);
        assert_eq!((d.replies, d.debug_cases, d.sent, d.edited), (2, 1, 2, 1));
        assert_eq!(d.minutes_saved, 14.0);
        assert_eq!(d.days.len(), 7);
        assert_eq!(d.days.last().unwrap().replies, 2);
        assert_eq!(d.issue_types[0].label, "CSS & layout");
    }

    #[test]
    fn buckets_issue_types() {
        assert_eq!(issue_type(&["casing".into()], ""), "Import casing");
        assert_eq!(issue_type(&[], "The media query hides the navbar"), "CSS & layout");
    }
}
