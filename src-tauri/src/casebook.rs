//! Issue history and the saved fix library (local SQLite, FTS5 search).
//!
//! - Issue history: every GitHub Debug diagnosis is stored with its repo,
//!   conversation and reply, so repeated problems are recognized.
//! - Fix library: reusable fixes you save (or promote from a diagnosis);
//!   the most relevant ones are offered to the analysis as known fixes.

use crate::db::{self, Db};
use crate::retrieval::fts_query;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    #[serde(default)]
    pub id: Option<i64>,
    pub title: String,
    pub problem: String,
    pub solution: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub uses: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRecord {
    pub id: i64,
    pub repo: String,
    pub issue: String,
    pub project_type: String,
    pub status: String,
    pub confidence: String,
    pub summary: String,
    pub findings: String,
    pub reply: String,
    pub created_at: i64,
}

pub struct NewIssue<'a> {
    pub repo: &'a str,
    pub scope_key: Option<&'a str>,
    pub conversation_id: Option<i64>,
    pub issue: &'a str,
    pub project_type: &'a str,
    pub status: &'a str,
    pub confidence: &'a str,
    pub summary: &'a str,
    pub findings_json: &'a str,
}

// ----- issue history -------------------------------------------------------

pub fn insert_issue(db: &Db, i: &NewIssue) -> rusqlite::Result<i64> {
    db.execute(
        "INSERT INTO debug_issues (repo, scope_key, conversation_id, issue, project_type, status, confidence, summary, findings, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![i.repo, i.scope_key, i.conversation_id, i.issue, i.project_type, i.status, i.confidence, i.summary, i.findings_json, db::now()],
    )?;
    Ok(db.last_insert_rowid())
}

pub fn set_issue_reply(db: &Db, id: i64, reply: &str) -> rusqlite::Result<()> {
    db.execute("UPDATE debug_issues SET reply = ?2 WHERE id = ?1", params![id, reply])?;
    Ok(())
}

fn row_to_issue(r: &rusqlite::Row) -> rusqlite::Result<IssueRecord> {
    Ok(IssueRecord {
        id: r.get(0)?,
        repo: r.get(1)?,
        issue: r.get(2)?,
        project_type: r.get(3)?,
        status: r.get(4)?,
        confidence: r.get(5)?,
        summary: r.get(6)?,
        findings: r.get(7)?,
        reply: r.get(8)?,
        created_at: r.get(9)?,
    })
}

const ISSUE_COLS: &str = "id, repo, issue, project_type, status, confidence, summary, findings, reply, created_at";

pub fn list_issues(db: &Db, limit: usize) -> rusqlite::Result<Vec<IssueRecord>> {
    let mut stmt = db.prepare(&format!("SELECT {ISSUE_COLS} FROM debug_issues ORDER BY created_at DESC, id DESC LIMIT ?1"))?;
    let rows = stmt.query_map(params![limit as i64], row_to_issue)?;
    rows.collect()
}

pub fn delete_issue(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM debug_issues WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_issues(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM debug_issues;")
}

/// Past cases worth showing: earlier issues in the same repo, plus similar issues anywhere.
pub fn similar_issues(db: &Db, repo: &str, text: &str, limit: usize) -> Vec<IssueRecord> {
    let mut out: Vec<IssueRecord> = Vec::new();
    if !repo.is_empty() {
        if let Ok(mut stmt) = db.prepare(&format!(
            "SELECT {ISSUE_COLS} FROM debug_issues WHERE lower(repo) = lower(?1) ORDER BY created_at DESC LIMIT 2"
        )) {
            if let Ok(rows) = stmt.query_map(params![repo], row_to_issue) {
                out.extend(rows.flatten());
            }
        }
    }
    if let Some(q) = fts_query(text, &[]) {
        let sql = format!(
            "SELECT {} , bm25(debug_issues_fts, 3.0, 2.0, 1.0) AS score FROM debug_issues_fts
             JOIN debug_issues d ON d.id = debug_issues_fts.rowid
             WHERE debug_issues_fts MATCH ?1 ORDER BY score LIMIT 6",
            ISSUE_COLS.split(", ").map(|c| format!("d.{c}")).collect::<Vec<_>>().join(", ")
        );
        if let Ok(mut stmt) = db.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(params![q], |r| Ok((row_to_issue(r)?, r.get::<_, f64>(10)?))) {
                let hits: Vec<(IssueRecord, f64)> = rows.flatten().collect();
                let best = hits.first().map(|h| -h.1).unwrap_or(0.0);
                for (rec, score) in hits {
                    if -score >= best * 0.5 && !out.iter().any(|o| o.id == rec.id) {
                        out.push(rec);
                    }
                }
            }
        }
    }
    out.truncate(limit);
    out
}

// ----- fix library -----------------------------------------------------------

fn row_to_fix(r: &rusqlite::Row) -> rusqlite::Result<Fix> {
    Ok(Fix {
        id: Some(r.get(0)?),
        title: r.get(1)?,
        problem: r.get(2)?,
        solution: r.get(3)?,
        snippet: r.get(4)?,
        tags: r.get(5)?,
        project_type: r.get(6)?,
        uses: r.get(7)?,
    })
}

const FIX_COLS: &str = "id, title, problem, solution, snippet, tags, project_type, uses";

pub fn list_fixes(db: &Db) -> rusqlite::Result<Vec<Fix>> {
    let mut stmt = db.prepare(&format!("SELECT {FIX_COLS} FROM fixes ORDER BY uses DESC, updated_at DESC"))?;
    let rows = stmt.query_map([], row_to_fix)?;
    rows.collect()
}

pub fn save_fix(db: &Db, f: &Fix) -> rusqlite::Result<i64> {
    let t = db::now();
    match f.id {
        Some(id) => {
            db.execute(
                "UPDATE fixes SET title=?2, problem=?3, solution=?4, snippet=?5, tags=?6, project_type=?7, updated_at=?8 WHERE id=?1",
                params![id, f.title, f.problem, f.solution, f.snippet, f.tags, f.project_type, t],
            )?;
            Ok(id)
        }
        None => {
            db.execute(
                "INSERT INTO fixes (title, problem, solution, snippet, tags, project_type, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![f.title, f.problem, f.solution, f.snippet, f.tags, f.project_type, t],
            )?;
            Ok(db.last_insert_rowid())
        }
    }
}

pub fn delete_fix(db: &Db, id: i64) -> rusqlite::Result<()> {
    db.execute("DELETE FROM fixes WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_fixes(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM fixes;")
}

pub fn bump_fix(db: &Db, id: i64) {
    let _ = db.execute("UPDATE fixes SET uses = uses + 1 WHERE id = ?1", params![id]);
}

/// Most relevant saved fixes for an issue; fixes for the same project type rank higher.
pub fn relevant_fixes(db: &Db, text: &str, project_tags: &[String], limit: usize) -> Vec<Fix> {
    let Some(q) = fts_query(text, &[]) else { return Vec::new() };
    let sql = format!(
        "SELECT {}, bm25(fixes_fts, 4.0, 2.0, 1.0, 3.0) AS score FROM fixes_fts JOIN fixes f ON f.id = fixes_fts.rowid
         WHERE fixes_fts MATCH ?1 ORDER BY score LIMIT 12",
        FIX_COLS.split(", ").map(|c| format!("f.{c}")).collect::<Vec<_>>().join(", ")
    );
    let Ok(mut stmt) = db.prepare(&sql) else { return Vec::new() };
    let Ok(rows) = stmt.query_map(params![q], |r| Ok((row_to_fix(r)?, r.get::<_, f64>(8)?))) else { return Vec::new() };
    let mut hits: Vec<(Fix, f64)> = rows
        .flatten()
        .map(|(f, s)| {
            let pt = f.project_type.to_lowercase();
            let boost = if !pt.is_empty() && project_tags.iter().any(|t| pt.contains(t.as_str())) { 1.4 } else { 1.0 };
            (f, -s * boost)
        })
        .collect();
    hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let best = hits.first().map(|h| h.1).unwrap_or(0.0);
    hits.into_iter().filter(|(_, s)| *s >= best * 0.45).take(limit).map(|(f, _)| f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_history_and_fixes_round_trip() {
        let db = db::open_in_memory().unwrap();
        let id = insert_issue(
            &db,
            &NewIssue {
                repo: "jane/portfolio",
                scope_key: None,
                conversation_id: None,
                issue: "navbar disappears on mobile",
                project_type: "React · Vite",
                status: "found",
                confidence: "high",
                summary: "Media query hides the navbar",
                findings_json: "[]",
            },
        )
        .unwrap();
        set_issue_reply(&db, id, "Hey! Remove display: none :)").unwrap();
        let similar = similar_issues(&db, "other/repo", "my navbar is gone on phones", 3);
        assert_eq!(similar.len(), 1);
        assert_eq!(similar_issues(&db, "jane/portfolio", "unrelated words", 3).len(), 1); // same repo

        save_fix(&db, &Fix { id: None, title: "Navbar hidden in media query".into(), problem: "navbar display none on mobile".into(), solution: "Remove display:none".into(), snippet: String::new(), tags: "css, responsive".into(), project_type: "react".into(), uses: 0 }).unwrap();
        save_fix(&db, &Fix { id: None, title: "Module not found".into(), problem: "import path casing".into(), solution: "Match casing".into(), snippet: String::new(), tags: "imports".into(), project_type: String::new(), uses: 0 }).unwrap();
        let hits = relevant_fixes(&db, "the navbar is hidden on mobile", &["react".into()], 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Navbar hidden in media query");
    }
}
