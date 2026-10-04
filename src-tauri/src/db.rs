//! Local SQLite database (~/Library/Application Support/com.pastepilot.app/pastepilot.db).
//!
//! Holds conversation memory, reply examples and the knowledge base.
//! Settings stay in settings.json and the API key in the Keychain.
//! Full-text search (FTS5, porter stemming) gives sub-millisecond retrieval.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub type Db = Connection;

/// Schema migrations, applied in order. `PRAGMA user_version` records how many ran.
const MIGRATIONS: &[&str] = &[
    // 1: conversation memory, reply examples, knowledge base
    r#"
    CREATE TABLE conversations (
        id          INTEGER PRIMARY KEY,
        scope_key   TEXT    NOT NULL,             -- app + page URL, or app + window title without a URL, or 'manual'
        title       TEXT    NOT NULL DEFAULT '',
        closed      INTEGER NOT NULL DEFAULT 0,   -- 1 after "New conversation"
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL
    );
    CREATE INDEX idx_conversations_scope ON conversations(scope_key, closed, updated_at);

    CREATE TABLE messages (
        id               INTEGER PRIMARY KEY,
        conversation_id  INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        role             TEXT    NOT NULL CHECK (role IN ('student', 'agent')),
        content          TEXT    NOT NULL,
        mode             TEXT,
        created_at       INTEGER NOT NULL
    );
    CREATE INDEX idx_messages_conversation ON messages(conversation_id, id);

    CREATE TABLE reply_examples (
        id               INTEGER PRIMARY KEY,
        student_message  TEXT    NOT NULL,
        reply            TEXT    NOT NULL,
        category         TEXT    NOT NULL DEFAULT '',
        created_at       INTEGER NOT NULL,
        updated_at       INTEGER NOT NULL
    );
    CREATE VIRTUAL TABLE reply_examples_fts USING fts5(
        student_message, reply, category,
        content='reply_examples', content_rowid='id', tokenize='porter unicode61'
    );
    CREATE TRIGGER reply_examples_ai AFTER INSERT ON reply_examples BEGIN
        INSERT INTO reply_examples_fts(rowid, student_message, reply, category)
        VALUES (new.id, new.student_message, new.reply, new.category);
    END;
    CREATE TRIGGER reply_examples_ad AFTER DELETE ON reply_examples BEGIN
        INSERT INTO reply_examples_fts(reply_examples_fts, rowid, student_message, reply, category)
        VALUES ('delete', old.id, old.student_message, old.reply, old.category);
    END;
    CREATE TRIGGER reply_examples_au AFTER UPDATE ON reply_examples BEGIN
        INSERT INTO reply_examples_fts(reply_examples_fts, rowid, student_message, reply, category)
        VALUES ('delete', old.id, old.student_message, old.reply, old.category);
        INSERT INTO reply_examples_fts(rowid, student_message, reply, category)
        VALUES (new.id, new.student_message, new.reply, new.category);
    END;

    CREATE TABLE knowledge_base (
        id          INTEGER PRIMARY KEY,
        title       TEXT    NOT NULL,
        content     TEXT    NOT NULL,
        category    TEXT    NOT NULL DEFAULT '',
        tags        TEXT    NOT NULL DEFAULT '',   -- comma separated
        enabled     INTEGER NOT NULL DEFAULT 1,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL
    );
    CREATE VIRTUAL TABLE knowledge_fts USING fts5(
        title, content, category, tags,
        content='knowledge_base', content_rowid='id', tokenize='porter unicode61'
    );
    CREATE TRIGGER knowledge_ai AFTER INSERT ON knowledge_base BEGIN
        INSERT INTO knowledge_fts(rowid, title, content, category, tags)
        VALUES (new.id, new.title, new.content, new.category, new.tags);
    END;
    CREATE TRIGGER knowledge_ad AFTER DELETE ON knowledge_base BEGIN
        INSERT INTO knowledge_fts(knowledge_fts, rowid, title, content, category, tags)
        VALUES ('delete', old.id, old.title, old.content, old.category, old.tags);
    END;
    CREATE TRIGGER knowledge_au AFTER UPDATE ON knowledge_base BEGIN
        INSERT INTO knowledge_fts(knowledge_fts, rowid, title, content, category, tags)
        VALUES ('delete', old.id, old.title, old.content, old.category, old.tags);
        INSERT INTO knowledge_fts(rowid, title, content, category, tags)
        VALUES (new.id, new.title, new.content, new.category, new.tags);
    END;
    "#,
    // 2: issue history, fix library, clipboard history, analytics, feedback learning
    r#"
    CREATE TABLE debug_issues (
        id               INTEGER PRIMARY KEY,
        repo             TEXT    NOT NULL,           -- owner/repo ('' for screenshot-only cases)
        scope_key        TEXT,
        conversation_id  INTEGER,
        issue            TEXT    NOT NULL,
        project_type     TEXT    NOT NULL DEFAULT '',
        status           TEXT    NOT NULL,           -- found | uncertain
        confidence       TEXT    NOT NULL,           -- high | medium | low
        summary          TEXT    NOT NULL DEFAULT '',
        findings         TEXT    NOT NULL DEFAULT '[]',  -- JSON
        reply            TEXT    NOT NULL DEFAULT '',
        created_at       INTEGER NOT NULL
    );
    CREATE INDEX idx_debug_issues_repo ON debug_issues(repo, created_at);
    CREATE VIRTUAL TABLE debug_issues_fts USING fts5(
        issue, summary, findings,
        content='debug_issues', content_rowid='id', tokenize='porter unicode61'
    );
    CREATE TRIGGER debug_issues_ai AFTER INSERT ON debug_issues BEGIN
        INSERT INTO debug_issues_fts(rowid, issue, summary, findings) VALUES (new.id, new.issue, new.summary, new.findings);
    END;
    CREATE TRIGGER debug_issues_ad AFTER DELETE ON debug_issues BEGIN
        INSERT INTO debug_issues_fts(debug_issues_fts, rowid, issue, summary, findings)
        VALUES ('delete', old.id, old.issue, old.summary, old.findings);
    END;
    CREATE TRIGGER debug_issues_au AFTER UPDATE ON debug_issues BEGIN
        INSERT INTO debug_issues_fts(debug_issues_fts, rowid, issue, summary, findings)
        VALUES ('delete', old.id, old.issue, old.summary, old.findings);
        INSERT INTO debug_issues_fts(rowid, issue, summary, findings) VALUES (new.id, new.issue, new.summary, new.findings);
    END;

    CREATE TABLE fixes (
        id            INTEGER PRIMARY KEY,
        title         TEXT    NOT NULL,
        problem       TEXT    NOT NULL,
        solution      TEXT    NOT NULL,
        snippet       TEXT    NOT NULL DEFAULT '',
        tags          TEXT    NOT NULL DEFAULT '',
        project_type  TEXT    NOT NULL DEFAULT '',
        uses          INTEGER NOT NULL DEFAULT 0,
        created_at    INTEGER NOT NULL,
        updated_at    INTEGER NOT NULL
    );
    CREATE VIRTUAL TABLE fixes_fts USING fts5(
        title, problem, solution, tags,
        content='fixes', content_rowid='id', tokenize='porter unicode61'
    );
    CREATE TRIGGER fixes_ai AFTER INSERT ON fixes BEGIN
        INSERT INTO fixes_fts(rowid, title, problem, solution, tags) VALUES (new.id, new.title, new.problem, new.solution, new.tags);
    END;
    CREATE TRIGGER fixes_ad AFTER DELETE ON fixes BEGIN
        INSERT INTO fixes_fts(fixes_fts, rowid, title, problem, solution, tags)
        VALUES ('delete', old.id, old.title, old.problem, old.solution, old.tags);
    END;
    CREATE TRIGGER fixes_au AFTER UPDATE ON fixes BEGIN
        INSERT INTO fixes_fts(fixes_fts, rowid, title, problem, solution, tags)
        VALUES ('delete', old.id, old.title, old.problem, old.solution, old.tags);
        INSERT INTO fixes_fts(rowid, title, problem, solution, tags) VALUES (new.id, new.title, new.problem, new.solution, new.tags);
    END;

    CREATE TABLE clipboard_history (
        id          INTEGER PRIMARY KEY,
        kind        TEXT    NOT NULL CHECK (kind IN ('student', 'reply', 'copied')),
        content     TEXT    NOT NULL,
        source      TEXT    NOT NULL DEFAULT '',
        created_at  INTEGER NOT NULL
    );
    CREATE INDEX idx_clipboard_created ON clipboard_history(created_at);

    CREATE TABLE events (
        id          INTEGER PRIMARY KEY,
        kind        TEXT    NOT NULL,   -- reply | rewrite | debug | sent | example
        mode        TEXT,
        detail      TEXT,
        value       REAL,
        created_at  INTEGER NOT NULL
    );
    CREATE INDEX idx_events_kind ON events(kind, created_at);

    CREATE TABLE feedback (
        id               INTEGER PRIMARY KEY,
        mode             TEXT,
        student_message  TEXT    NOT NULL,
        generated        TEXT    NOT NULL,
        final            TEXT    NOT NULL,
        similarity       REAL    NOT NULL,
        created_at       INTEGER NOT NULL
    );
    "#,
    // 3: Assignment Review presets (example site + requirements per assignment)
    r#"
    CREATE TABLE assignment_presets (
        id            INTEGER PRIMARY KEY,
        name          TEXT    NOT NULL,
        example_url   TEXT    NOT NULL,
        requirements  TEXT    NOT NULL DEFAULT '',
        updated_at    INTEGER NOT NULL
    );
    "#,
];

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn open(path: &Path) -> rusqlite::Result<Db> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> rusqlite::Result<Db> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Conversations and messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Student,
    Agent,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Student => "student",
            Role::Agent => "agent",
        }
    }
    fn parse(s: &str) -> Role {
        if s == "agent" {
            Role::Agent
        } else {
            Role::Student
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub role: Role,
    pub content: String,
    pub mode: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInfo {
    pub id: i64,
    pub title: String,
    pub updated_at: i64,
    pub messages: Vec<Message>,
}

/// Most recent open conversation for this scope that was active after `since`.
pub fn find_active_conversation(db: &Db, scope_key: &str, since: i64) -> rusqlite::Result<Option<i64>> {
    db.query_row(
        "SELECT id FROM conversations WHERE scope_key = ?1 AND closed = 0 AND updated_at >= ?2
         ORDER BY updated_at DESC LIMIT 1",
        params![scope_key, since],
        |r| r.get(0),
    )
    .optional()
}

pub fn create_conversation(db: &Db, scope_key: &str, title: &str) -> rusqlite::Result<i64> {
    let t = now();
    db.execute(
        "INSERT INTO conversations (scope_key, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![scope_key, title, t],
    )?;
    Ok(db.last_insert_rowid())
}

pub fn touch_conversation(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("UPDATE conversations SET updated_at = ?2 WHERE id = ?1", params![id, now()])?;
    Ok(())
}

pub fn close_conversation(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("UPDATE conversations SET closed = 1 WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn close_scope(db: &Db, scope_key: &str) -> rusqlite::Result<()> {
    db.execute("UPDATE conversations SET closed = 1 WHERE scope_key = ?1", params![scope_key])?;
    Ok(())
}

pub fn delete_conversation(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_conversations(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM messages; DELETE FROM conversations; VACUUM;")
}

pub fn conversation_counts(db: &Db) -> rusqlite::Result<(i64, i64)> {
    let convs = db.query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0))?;
    let msgs = db.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    Ok((convs, msgs))
}

pub fn conversation_info(db: &Db, id: i64) -> rusqlite::Result<Option<ConversationInfo>> {
    let head = db
        .query_row("SELECT title, updated_at FROM conversations WHERE id = ?1", params![id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .optional()?;
    let Some((title, updated_at)) = head else { return Ok(None) };
    let mut stmt = db.prepare(
        "SELECT id, role, content, mode, created_at FROM messages WHERE conversation_id = ?1 ORDER BY id",
    )?;
    let messages = stmt.query_map(params![id], row_to_message)?.collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ConversationInfo { id, title, updated_at, messages }))
}

fn row_to_message(r: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        role: Role::parse(&r.get::<_, String>(1)?),
        content: r.get(2)?,
        mode: r.get(3)?,
        created_at: r.get(4)?,
    })
}

/// Newest-first messages of a conversation with id < `before_id`.
pub fn messages_before(db: &Db, conversation_id: i64, before_id: i64, limit: usize) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = db.prepare_cached(
        "SELECT id, role, content, mode, created_at FROM messages
         WHERE conversation_id = ?1 AND id < ?2 ORDER BY id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![conversation_id, before_id, limit as i64], row_to_message)?;
    rows.collect()
}

/// The last `n` messages, newest first.
pub fn last_messages(db: &Db, conversation_id: i64, n: usize) -> rusqlite::Result<Vec<Message>> {
    messages_before(db, conversation_id, i64::MAX, n)
}

pub fn insert_message(db: &Db, conversation_id: i64, role: Role, content: &str, mode: Option<&str>) -> rusqlite::Result<i64> {
    db.execute(
        "INSERT INTO messages (conversation_id, role, content, mode, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![conversation_id, role.as_str(), content, mode, now()],
    )?;
    let id = db.last_insert_rowid();
    touch_conversation(db, conversation_id)?;
    Ok(id)
}

pub fn update_message(db: &Db, id: i64, content: &str) -> rusqlite::Result<()> {
    db.execute("UPDATE messages SET content = ?2 WHERE id = ?1", params![id, content])?;
    Ok(())
}

pub fn set_message_mode(db: &Db, id: i64, mode: &str) -> rusqlite::Result<()> {
    db.execute("UPDATE messages SET mode = ?2 WHERE id = ?1", params![id, mode])?;
    Ok(())
}

pub fn delete_message(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM messages WHERE id = ?1", params![id])?;
    Ok(())
}

/// Mode of the most recent student message in the conversation (for "sticky" modes).
pub fn last_mode(db: &Db, conversation_id: i64, before_id: i64) -> rusqlite::Result<Option<String>> {
    db.query_row(
        "SELECT mode FROM messages WHERE conversation_id = ?1 AND id < ?2 AND role = 'student' AND mode IS NOT NULL
         ORDER BY id DESC LIMIT 1",
        params![conversation_id, before_id],
        |r| r.get(0),
    )
    .optional()
}

// ---------------------------------------------------------------------------
// Reply examples
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyExample {
    #[serde(default)]
    pub id: Option<i64>,
    pub student_message: String,
    pub reply: String,
    #[serde(default)]
    pub category: String,
}

fn row_to_example(r: &rusqlite::Row) -> rusqlite::Result<ReplyExample> {
    Ok(ReplyExample { id: Some(r.get(0)?), student_message: r.get(1)?, reply: r.get(2)?, category: r.get(3)? })
}

pub fn list_examples(db: &Db) -> rusqlite::Result<Vec<ReplyExample>> {
    let mut stmt =
        db.prepare("SELECT id, student_message, reply, category FROM reply_examples ORDER BY updated_at DESC, id DESC")?;
    let rows = stmt.query_map([], row_to_example)?;
    rows.collect()
}

pub fn save_example(db: &Db, ex: &ReplyExample) -> rusqlite::Result<i64> {
    let t = now();
    match ex.id {
        Some(id) => {
            db.execute(
                "UPDATE reply_examples SET student_message = ?2, reply = ?3, category = ?4, updated_at = ?5 WHERE id = ?1",
                params![id, ex.student_message, ex.reply, ex.category, t],
            )?;
            Ok(id)
        }
        None => {
            db.execute(
                "INSERT INTO reply_examples (student_message, reply, category, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![ex.student_message, ex.reply, ex.category, t],
            )?;
            Ok(db.last_insert_rowid())
        }
    }
}

pub fn delete_example(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM reply_examples WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_examples(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM reply_examples;")
}

/// BM25-ranked matches (lower score = better). Student message weighs most.
pub fn search_examples(db: &Db, fts_query: &str, limit: usize) -> rusqlite::Result<Vec<(ReplyExample, f64)>> {
    let mut stmt = db.prepare_cached(
        "SELECT e.id, e.student_message, e.reply, e.category, bm25(reply_examples_fts, 4.0, 1.0, 1.0) AS score
         FROM reply_examples_fts JOIN reply_examples e ON e.id = reply_examples_fts.rowid
         WHERE reply_examples_fts MATCH ?1 ORDER BY score LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, limit as i64], |r| Ok((row_to_example(r)?, r.get::<_, f64>(4)?)))?;
    rows.collect()
}

pub fn recent_examples(db: &Db, category: Option<&str>, limit: usize) -> rusqlite::Result<Vec<ReplyExample>> {
    let mut stmt = db.prepare_cached(
        "SELECT id, student_message, reply, category FROM reply_examples
         WHERE ?1 IS NULL OR lower(category) = lower(?1) ORDER BY updated_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![category, limit as i64], row_to_example)?;
    rows.collect()
}

pub fn all_example_replies(db: &Db) -> rusqlite::Result<Vec<String>> {
    let mut stmt = db.prepare("SELECT reply FROM reply_examples")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect()
}

// ---------------------------------------------------------------------------
// Knowledge base
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KbEntry {
    #[serde(default)]
    pub id: Option<i64>,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

fn row_to_kb(r: &rusqlite::Row) -> rusqlite::Result<KbEntry> {
    Ok(KbEntry {
        id: Some(r.get(0)?),
        title: r.get(1)?,
        content: r.get(2)?,
        category: r.get(3)?,
        tags: r.get(4)?,
        enabled: r.get::<_, i64>(5)? != 0,
    })
}

pub fn list_kb(db: &Db) -> rusqlite::Result<Vec<KbEntry>> {
    let mut stmt = db.prepare(
        "SELECT id, title, content, category, tags, enabled FROM knowledge_base ORDER BY category, title COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], row_to_kb)?;
    rows.collect()
}

pub fn save_kb(db: &Db, e: &KbEntry) -> rusqlite::Result<i64> {
    let t = now();
    match e.id {
        Some(id) => {
            db.execute(
                "UPDATE knowledge_base SET title = ?2, content = ?3, category = ?4, tags = ?5, enabled = ?6, updated_at = ?7
                 WHERE id = ?1",
                params![id, e.title, e.content, e.category, e.tags, e.enabled as i64, t],
            )?;
            Ok(id)
        }
        None => {
            db.execute(
                "INSERT INTO knowledge_base (title, content, category, tags, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![e.title, e.content, e.category, e.tags, e.enabled as i64, t],
            )?;
            Ok(db.last_insert_rowid())
        }
    }
}

pub fn set_kb_enabled(db: &Db, id: i64, enabled: bool) -> rusqlite::Result<()> {
    db.execute("UPDATE knowledge_base SET enabled = ?2 WHERE id = ?1", params![id, enabled as i64])?;
    Ok(())
}

pub fn delete_kb(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM knowledge_base WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_kb(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM knowledge_base;")
}

/// BM25-ranked enabled entries (lower score = better). Title and tags weigh most.
pub fn search_kb(db: &Db, fts_query: &str, limit: usize) -> rusqlite::Result<Vec<(KbEntry, f64)>> {
    let mut stmt = db.prepare_cached(
        "SELECT k.id, k.title, k.content, k.category, k.tags, k.enabled, bm25(knowledge_fts, 5.0, 1.0, 2.0, 3.0) AS score
         FROM knowledge_fts JOIN knowledge_base k ON k.id = knowledge_fts.rowid
         WHERE knowledge_fts MATCH ?1 AND k.enabled = 1 ORDER BY score LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, limit as i64], |r| Ok((row_to_kb(r)?, r.get::<_, f64>(6)?)))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_round_trips() {
        let db = open_in_memory().unwrap();
        let c = create_conversation(&db, "scope", "Ticket").unwrap();
        let m1 = insert_message(&db, c, Role::Student, "npm install fails", Some("technical")).unwrap();
        let m2 = insert_message(&db, c, Role::Agent, "Can you send the error?", None).unwrap();
        let m3 = insert_message(&db, c, Role::Student, "I sent it above", None).unwrap();
        let before = messages_before(&db, c, m3, 10).unwrap();
        assert_eq!(before.iter().map(|m| m.id).collect::<Vec<_>>(), vec![m2, m1]);
        assert_eq!(last_mode(&db, c, m3).unwrap().as_deref(), Some("technical"));
        assert_eq!(find_active_conversation(&db, "scope", 0).unwrap(), Some(c));
        close_conversation(&db, c).unwrap();
        assert_eq!(find_active_conversation(&db, "scope", 0).unwrap(), None);
        delete_conversation(&db, c).unwrap();
        assert_eq!(conversation_counts(&db).unwrap(), (0, 0)); // messages cascade
    }

    #[test]
    fn fts_search_and_sync() {
        let db = open_in_memory().unwrap();
        let id = save_kb(
            &db,
            &KbEntry {
                id: None,
                title: "Refund policy".into(),
                content: "Refunds within 14 days of purchase.".into(),
                category: "billing".into(),
                tags: "refund, money back".into(),
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(search_kb(&db, "\"refunds\"", 5).unwrap().len(), 1); // porter: refunds ~ refund
        set_kb_enabled(&db, id, false).unwrap();
        assert!(search_kb(&db, "\"refund\"", 5).unwrap().is_empty());

        let ex = save_example(
            &db,
            &ReplyExample { id: None, student_message: "npm install error".into(), reply: "Try deleting node_modules!".into(), category: "technical".into() },
        )
        .unwrap();
        assert_eq!(search_examples(&db, "\"npm\"", 3).unwrap().len(), 1);
        save_example(
            &db,
            &ReplyExample { id: Some(ex), student_message: "login problem".into(), reply: "Reset it :)".into(), category: "".into() },
        )
        .unwrap();
        assert!(search_examples(&db, "\"npm\"", 3).unwrap().is_empty());
        assert_eq!(search_examples(&db, "\"login\"", 3).unwrap().len(), 1);
    }
}
