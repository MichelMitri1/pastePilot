//! Shared, read-only access to a student's repository for the AI features
//! (GitHub Debug and Assignment Review): open the tree, read only the files
//! that matter within size limits, and turn findings into real code snippets.
//! Nothing here executes or modifies repository code.

use crate::github::{self, GhError, RepoRef, RepoTree, TreeEntry};
use crate::repo_search as search;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    pub max_file_chars: usize,
    pub max_total_chars: usize,
}

pub struct Workspace {
    pub repo: RepoRef,
    pub tree: RepoTree,
    pub by_path: HashMap<String, TreeEntry>,
    /// Readable source files (candidates for reading).
    pub all_paths: HashSet<String>,
    /// Every file in the tree, including images and other binaries (for "does this path exist?").
    pub every_path: HashSet<String>,
    /// (path, contents read, truncated?)
    pub files: Vec<(String, Arc<str>, bool)>,
    pub total_chars: usize,
    pub notes: Vec<String>,
    pub limits: Limits,
}

/// Lists the repository (default branch unless the URL named one) and keeps the readable files.
pub async fn open(repo: RepoRef, limits: Limits) -> Result<(Workspace, Vec<TreeEntry>), String> {
    let gh = github::shared();
    let mut notes = Vec::new();
    let tree = match &repo.branch {
        Some(branch) => match gh.tree(&repo, branch).await {
            Ok(t) => t,
            Err(GhError::NotFound) => {
                let t = gh.tree(&repo, "HEAD").await.map_err(|e| e.to_string())?;
                notes.push(format!("Branch \"{branch}\" wasn't found, so the default branch was used."));
                t
            }
            Err(e) => return Err(e.to_string()),
        },
        None => gh.tree(&repo, "HEAD").await.map_err(|e| e.to_string())?,
    };
    if tree.truncated {
        notes.push("This repository is very large; only part of its file list was available.".into());
    }
    let candidates: Vec<TreeEntry> = search::candidates(&tree.entries).into_iter().cloned().collect();
    if candidates.is_empty() {
        return Err("No readable code files were found in this repository.".into());
    }
    let ws = Workspace {
        by_path: candidates.iter().map(|e| (e.path.clone(), e.clone())).collect(),
        all_paths: candidates.iter().map(|e| e.path.clone()).collect(),
        every_path: tree.entries.iter().map(|e| e.path.clone()).collect(),
        repo,
        tree,
        files: Vec::new(),
        total_chars: 0,
        notes,
        limits,
    };
    Ok((ws, candidates))
}

impl Workspace {
    pub fn has(&self, path: &str) -> bool {
        self.files.iter().any(|(p, _, _)| p == path)
    }

    pub fn content(&self, path: &str) -> Option<&str> {
        self.files.iter().find(|(p, _, _)| p == path).map(|(_, c, _)| c.as_ref())
    }

    /// (path, contents) of everything read so far.
    pub fn read(&self) -> Vec<(String, Arc<str>)> {
        self.files.iter().map(|(p, c, _)| (p.clone(), c.clone())).collect()
    }

    pub fn package_json(&self) -> Option<Value> {
        self.files
            .iter()
            .filter(|(p, _, _)| search::basename(p) == "package.json")
            .min_by_key(|(p, _, _)| p.len())
            .and_then(|(_, c, _)| serde_json::from_str(c).ok())
    }

    pub fn blob_url(&self, path: &str, lines: Option<(u32, u32)>) -> String {
        github::blob_url(&self.repo, &self.tree.git_ref, path, lines)
    }

    /// Fetches files in parallel (cached by SHA), within the file and size limits.
    pub async fn fetch(&mut self, paths: Vec<String>) {
        let gh = github::shared();
        let wanted: Vec<TreeEntry> = paths
            .into_iter()
            .filter(|p| !self.has(p))
            .filter_map(|p| self.by_path.get(&p).cloned())
            .take(self.limits.max_files.saturating_sub(self.files.len()))
            .collect();
        let results = futures_util::future::join_all(wanted.iter().map(|e| gh.file(&self.repo, &self.tree.git_ref, e))).await;
        for (entry, result) in wanted.into_iter().zip(results) {
            match result {
                Ok(text) => {
                    let room = self.limits.max_total_chars.saturating_sub(self.total_chars);
                    if room < 500 {
                        self.notes.push(format!("Skipped {} (context limit reached).", entry.path));
                        continue;
                    }
                    let limit = self.limits.max_file_chars.min(room);
                    let truncated = text.len() > limit;
                    let kept: Arc<str> = if truncated { truncate_at_line(&text, limit).into() } else { text };
                    self.total_chars += kept.len();
                    if truncated {
                        self.notes.push(format!("{} is long; only the first part was read.", entry.path));
                    }
                    self.files.push((entry.path, kept, truncated));
                }
                Err(GhError::Binary) => self.notes.push(format!("Skipped {} (not a text file).", entry.path)),
                Err(e) => self.notes.push(format!("Couldn't read {}: {e}", entry.path)),
            }
        }
    }

    /// File contents with line numbers, fenced as untrusted, for AI prompts.
    pub fn numbered_files(&self) -> String {
        let mut u = String::with_capacity(self.total_chars + 1024);
        for (path, content, truncated) in &self.files {
            let lines: Vec<&str> = content.lines().collect();
            u.push_str(&format!("\n<<<FILE {path} ({} lines{})>>>\n", lines.len(), if *truncated { ", truncated" } else { "" }));
            for (i, line) in lines.iter().enumerate() {
                u.push_str(&format!("{:>4}| {}\n", i + 1, line));
            }
            u.push_str("<<<END FILE>>>\n");
        }
        u
    }
}

pub fn truncate_at_line(text: &str, limit: usize) -> String {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let cut = text[..end].rfind('\n').unwrap_or(end);
    text[..cut].to_string()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub start_line: u32,
    pub lines: Vec<String>,
    pub highlight_start: u32,
    pub highlight_end: u32,
}

/// A few lines around a finding, taken from the real file.
pub fn snippet(content: &str, start: u32, end: u32) -> Snippet {
    let from = start.saturating_sub(2).max(1);
    let to = (end + 2).min(from + 13);
    let lines = content
        .lines()
        .enumerate()
        .filter(|(i, _)| (*i as u32 + 1) >= from && (*i as u32 + 1) <= to)
        .map(|(_, l)| {
            let l = l.trim_end();
            if l.chars().count() > 180 {
                format!("{}…", l.chars().take(180).collect::<String>())
            } else {
                l.to_string()
            }
        })
        .collect();
    Snippet { start_line: from, lines, highlight_start: start, highlight_end: end }
}

/// Exactly lines start..=end of a file.
pub fn real_lines(content: &str, start: u32, end: u32) -> String {
    content.lines().skip(start as usize - 1).take((end - start + 1) as usize).collect::<Vec<_>>().join("\n")
}

/// A positive line number from JSON (number or numeric string).
pub fn num(v: &Value) -> Option<u32> {
    v.as_u64().map(|n| n as u32).or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).filter(|n| *n > 0)
}

/// Code fence language for a path.
pub fn language(path: &str) -> String {
    match path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).as_deref() {
        Some("js" | "mjs" | "cjs") => "javascript",
        Some("jsx") => "jsx",
        Some("ts") => "typescript",
        Some("tsx") => "tsx",
        Some("css") => "css",
        Some("scss") => "scss",
        Some("html" | "htm") => "html",
        Some("json") => "json",
        Some("vue") => "vue",
        Some("py") => "python",
        _ => "",
    }
    .to_string()
}

/// Removes ``` fences the model sometimes adds around code.
pub fn strip_fences(s: &str) -> String {
    let t = s.trim_matches('\n');
    if let Some(rest) = t.trim_start().strip_prefix("```") {
        let body = rest.split_once('\n').map(|(_, b)| b).unwrap_or("");
        return body.trim_end().trim_end_matches("```").trim_end().to_string();
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_and_lines_come_from_the_file() {
        let content = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let s = snippet(&content, 10, 11);
        assert_eq!(s.start_line, 8);
        assert_eq!(s.lines.last().map(String::as_str), Some("line 13"));
        assert_eq!(real_lines(&content, 3, 4), "line 3\nline 4");
        assert_eq!(truncate_at_line("aaa\nbbb\nccc", 6), "aaa");
        assert_eq!(strip_fences("```css\n.a {}\n```"), ".a {}");
    }
}
