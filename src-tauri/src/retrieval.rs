//! Picks the few knowledge-base entries and reply examples worth sending.
//!
//! Uses SQLite FTS5 BM25 ranking with porter stemming: sub-millisecond, fully
//! local, no extra network call. Results are filtered relative to the best
//! match so weak keyword overlaps don't pad the prompt.

use crate::db::{self, Db, KbEntry, ReplyExample};
use crate::modes::Mode;
use std::collections::HashSet;

const MAX_QUERY_TERMS: usize = 24;
/// Keep results scoring at least this fraction of the best match.
const RELATIVE_CUTOFF: f64 = 0.35;
const KB_ENTRY_MAX_CHARS: usize = 1200;
const KB_TOTAL_MAX_CHARS: usize = 3000;
const EXAMPLE_SIDE_MAX_CHARS: usize = 500;

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "your", "all", "any", "can", "had", "her", "was", "one", "our",
    "out", "has", "have", "him", "his", "how", "its", "may", "who", "did", "get", "got", "just", "now", "too", "use",
    "she", "they", "them", "then", "than", "this", "that", "with", "from", "what", "when", "where", "which", "will",
    "would", "could", "should", "there", "their", "about", "into", "also", "been", "were", "here", "some", "very",
    "more", "much", "like", "want", "need", "know", "thanks", "thank", "please", "hello", "hey", "help", "really",
    "does", "doesn", "don", "isn", "aren", "wasn", "didn", "can", "cant", "won", "still", "even", "only", "why",
    "i'm", "im", "ive", "i've", "it's", "dont", "able", "after", "before", "because", "being", "make", "made",
];

/// Builds an FTS5 OR-query from the message's distinctive words (plus optional extra terms).
pub fn fts_query(text: &str, extra: &[&str]) -> Option<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut terms: Vec<String> = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let w = word.to_lowercase();
        if w.chars().count() < 3 || stop.contains(w.as_str()) || w.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if seen.insert(w.clone()) {
            terms.push(w);
        }
        if terms.len() >= MAX_QUERY_TERMS {
            break;
        }
    }
    for e in extra {
        if seen.insert((*e).to_string()) {
            terms.push((*e).to_string());
        }
    }
    if terms.is_empty() {
        return None;
    }
    // Quoted terms: FTS5 syntax characters in user text can't break the query.
    Some(terms.iter().map(|t| format!("\"{}\"", t.replace('"', ""))).collect::<Vec<_>>().join(" OR "))
}

fn category_matches(category: &str, mode: Mode) -> bool {
    let c = category.trim().to_lowercase();
    !c.is_empty() && (c == mode.id() || c.contains(mode.id()))
}

/// Applies a mode boost, drops weak matches, returns best-first.
fn rank<T>(mut hits: Vec<(T, f64)>, mode: Mode, category: impl Fn(&T) -> &str) -> Vec<T> {
    // BM25 from SQLite is negative; more negative is better. Flip to positive.
    for (item, score) in hits.iter_mut() {
        *score = -*score;
        if category_matches(category(item), mode) {
            *score *= 1.5;
        }
    }
    hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let best = hits.first().map(|h| h.1).unwrap_or(0.0);
    hits.into_iter().filter(|(_, s)| *s > 0.0 && *s >= best * RELATIVE_CUTOFF).map(|(t, _)| t).collect()
}

pub fn knowledge(db: &Db, text: &str, mode: Mode, limit: usize) -> Vec<KbEntry> {
    if limit == 0 {
        return Vec::new();
    }
    let Some(query) = fts_query(text, mode.search_expansion()) else { return Vec::new() };
    let hits = db::search_kb(db, &query, limit * 4).unwrap_or_default();
    let mut total = 0;
    rank(hits, mode, |e: &KbEntry| e.category.as_str())
        .into_iter()
        .take(limit)
        .take_while(|e| {
            total += e.content.len().min(KB_ENTRY_MAX_CHARS);
            total <= KB_TOTAL_MAX_CHARS
        })
        .collect()
}

pub fn examples(db: &Db, text: &str, mode: Mode, limit: usize) -> Vec<ReplyExample> {
    if limit == 0 {
        return Vec::new();
    }
    let mut picked: Vec<ReplyExample> = match fts_query(text, &[]) {
        Some(q) => rank(db::search_examples(db, &q, limit * 4).unwrap_or_default(), mode, |e: &ReplyExample| {
            e.category.as_str()
        })
        .into_iter()
        .take(limit)
        .collect(),
        None => Vec::new(),
    };
    // With no close match, a couple of recent examples still teach tone and length.
    let target = limit.min(2);
    if picked.len() < target {
        let want = target - picked.len();
        let ids: HashSet<Option<i64>> = picked.iter().map(|e| e.id).collect();
        let mut recent = db::recent_examples(db, Some(mode.id()), want + 2).unwrap_or_default();
        if recent.is_empty() {
            recent = db::recent_examples(db, None, want + 2).unwrap_or_default();
        }
        picked.extend(recent.into_iter().filter(|e| !ids.contains(&e.id)).take(want));
    }
    picked
}

pub fn truncate(text: &str, max_chars: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    let cut: String = t.chars().take(max_chars).collect();
    format!("{}…", cut.trim_end())
}

pub fn format_knowledge(entries: &[KbEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            let cat = if e.category.trim().is_empty() { String::new() } else { format!(" ({})", e.category.trim()) };
            format!("## {}{}\n{}", e.title.trim(), cat, truncate(&e.content, KB_ENTRY_MAX_CHARS))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn format_examples(examples: &[ReplyExample]) -> String {
    examples
        .iter()
        .map(|e| {
            format!(
                "Student: {}\nAgent: {}",
                truncate(&e.student_message, EXAMPLE_SIDE_MAX_CHARS),
                truncate(&e.reply, EXAMPLE_SIDE_MAX_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n")
}

/// A short, measured description of how the agent writes, computed from all
/// saved example replies. Rebuilt only when examples change.
pub fn style_profile(replies: &[String]) -> Option<String> {
    if replies.len() < 3 {
        return None;
    }
    let mut words: Vec<usize> = replies.iter().map(|r| r.split_whitespace().count()).collect();
    words.sort_unstable();
    let n = replies.len() as f64;
    let median = words[words.len() / 2];
    let p25 = words[words.len() / 4];
    let p75 = words[(words.len() * 3) / 4];
    let smiley = replies.iter().filter(|r| r.contains(":)") || r.contains(":D")).count() as f64 / n;
    let exclaims = replies.iter().map(|r| r.matches('!').count()).sum::<usize>() as f64 / n;
    let questions = replies.iter().filter(|r| r.contains('?')).count() as f64 / n;
    let multi_para = replies.iter().filter(|r| r.contains("\n\n") || r.lines().count() > 2).count() as f64 / n;

    let mut lines = vec![format!(
        "Typical reply length: about {median} words (usually {p25} to {p75})."
    )];
    lines.push(format!("Uses :) in about {:.0}% of replies.", smiley * 100.0));
    lines.push(format!("Averages {:.1} exclamation marks per reply.", exclaims));
    lines.push(format!("{:.0}% of replies ask the student a question.", questions * 100.0));
    lines.push(if multi_para > 0.5 {
        "Often splits replies into short paragraphs or steps.".to_string()
    } else {
        "Usually writes a single short paragraph.".to_string()
    });
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_safe_queries() {
        let q = fts_query("My npm install isn't working!! \"quote\" OR (x)", &[]).unwrap();
        assert!(q.contains("\"npm\""));
        assert!(q.contains("\"install\""));
        assert!(!q.contains("\"isn\""));
        assert!(fts_query("hi ok", &[]).is_none());
    }

    #[test]
    fn retrieves_relevant_entries_only() {
        let db = crate::db::open_in_memory().unwrap();
        for (title, content, cat) in [
            ("Refund policy", "Full refunds within 14 days of purchase.", "billing"),
            ("Reset password", "Use the Forgot password link on the login page.", "access"),
            ("Course length", "The bootcamp runs for 12 weeks.", "course"),
        ] {
            crate::db::save_kb(
                &db,
                &KbEntry { id: None, title: title.into(), content: content.into(), category: cat.into(), tags: String::new(), enabled: true },
            )
            .unwrap();
        }
        let hits = knowledge(&db, "Can I get my money back?", Mode::Billing, 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Refund policy");
        let hits = knowledge(&db, "I forgot my password and can't log in", Mode::Technical, 3);
        assert_eq!(hits[0].title, "Reset password");
    }

    #[test]
    fn style_profile_needs_a_few_examples() {
        assert!(style_profile(&["Hi!".into()]).is_none());
        let p = style_profile(&["Hey! Try this :)".into(), "Sure thing!".into(), "Can you send the error?".into()]).unwrap();
        assert!(p.contains("33%"));
    }
}
