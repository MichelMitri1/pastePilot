//! Feedback learning: when the reply you actually send differs a lot from the
//! draft, the pair is stored locally and turned into concrete style rules
//! ("make replies ~30% shorter", "avoid 'feel free to'"). Those rules are
//! added to the cached system prompt. Optionally the edited reply also becomes
//! a reply example. Nothing leaves your Mac except as part of a normal prompt.

use crate::db::{self, Db};
use rusqlite::params;
use std::collections::HashMap;

/// Below this similarity the edit counts as significant.
pub const SIGNIFICANT: f64 = 0.85;
const MIN_SAMPLES: usize = 3;

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(|w| w.to_lowercase()).take(400).collect()
}

/// 1.0 = identical, 0.0 = nothing in common (word-level edit distance).
pub fn similarity(a: &str, b: &str) -> f64 {
    let (x, y) = (words(a), words(b));
    if x.is_empty() && y.is_empty() {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=y.len()).collect();
    for i in 1..=x.len() {
        let mut cur = vec![i; y.len() + 1];
        for j in 1..=y.len() {
            let cost = if x[i - 1] == y[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    1.0 - prev[y.len()] as f64 / x.len().max(y.len()) as f64
}

pub fn record(db: &Db, mode: Option<&str>, student: &str, generated: &str, final_text: &str, similarity: f64) {
    let _ = db.execute(
        "INSERT INTO feedback (mode, student_message, generated, final, similarity, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![mode, student, generated, final_text, similarity, db::now()],
    );
}

pub fn count(db: &Db) -> i64 {
    db.query_row("SELECT COUNT(*) FROM feedback", [], |r| r.get(0)).unwrap_or(0)
}

pub fn clear(db: &Db) -> rusqlite::Result<()> {
    db.execute_batch("DELETE FROM feedback;")
}

fn recent_pairs(db: &Db) -> Vec<(String, String)> {
    let Ok(mut stmt) = db.prepare("SELECT generated, final FROM feedback ORDER BY id DESC LIMIT 60") else { return Vec::new() };
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

fn starts_with_greeting(s: &str) -> bool {
    let first = s.trim_start().split(|c: char| !c.is_alphabetic()).next().unwrap_or("").to_lowercase();
    matches!(first.as_str(), "hey" | "hi" | "hello" | "hiya" | "heya")
}

fn has_list(s: &str) -> bool {
    s.lines().filter(|l| {
        let t = l.trim_start();
        t.starts_with("- ") || t.starts_with("* ") || t.starts_with("• ") || t.split_once(". ").is_some_and(|(n, _)| n.parse::<u32>().is_ok())
    }).count() >= 2
}

fn trigrams(s: &str) -> std::collections::HashSet<String> {
    let w: Vec<String> = s
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    w.windows(3).map(|t| t.join(" ")).collect()
}

/// Style rules learned from how you edit drafts. None until there's enough evidence.
pub fn edit_profile(db: &Db) -> Option<String> {
    let pairs = recent_pairs(db);
    if pairs.len() < MIN_SAMPLES {
        return None;
    }
    let mut rules: Vec<String> = Vec::new();
    let rate = |hits: usize, total: usize| if total >= MIN_SAMPLES { hits as f64 / total as f64 } else { 0.0 };

    // Length.
    let mut ratios: Vec<f64> = pairs
        .iter()
        .filter(|(g, _)| !words(g).is_empty())
        .map(|(g, f)| words(f).len() as f64 / words(g).len() as f64)
        .collect();
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(median) = ratios.get(ratios.len() / 2) {
        if *median <= 0.8 {
            rules.push(format!("Make replies about {:.0}% shorter than a first draft would be.", (1.0 - median) * 100.0));
        } else if *median >= 1.25 {
            rules.push("Give a bit more detail than a first draft would.".into());
        }
    }

    // Greetings.
    let with_greeting: Vec<_> = pairs.iter().filter(|(g, _)| starts_with_greeting(g)).collect();
    let removed = with_greeting.iter().filter(|(_, f)| !starts_with_greeting(f)).count();
    let without: Vec<_> = pairs.iter().filter(|(g, _)| !starts_with_greeting(g)).collect();
    let added = without.iter().filter(|(_, f)| starts_with_greeting(f)).count();
    if rate(removed, with_greeting.len()) >= 0.6 {
        rules.push("Don't open with a greeting line.".into());
    } else if rate(added, without.len()) >= 0.6 {
        rules.push("Open with a short greeting like \"Hey!\".".into());
    }

    // :) and exclamation marks.
    let smiley = |s: &str| s.contains(":)") || s.contains(":D");
    let g_smiley: Vec<_> = pairs.iter().filter(|(g, _)| smiley(g)).collect();
    let g_plain: Vec<_> = pairs.iter().filter(|(g, _)| !smiley(g)).collect();
    if rate(g_smiley.iter().filter(|(_, f)| !smiley(f)).count(), g_smiley.len()) >= 0.6 {
        rules.push("Use :) less often.".into());
    } else if rate(g_plain.iter().filter(|(_, f)| smiley(f)).count(), g_plain.len()) >= 0.4 {
        rules.push("Add :) a bit more often when it fits.".into());
    }
    let delta: f64 = pairs.iter().map(|(g, f)| f.matches('!').count() as f64 - g.matches('!').count() as f64).sum::<f64>() / pairs.len() as f64;
    if delta <= -0.8 {
        rules.push("Use fewer exclamation marks.".into());
    } else if delta >= 0.8 {
        rules.push("Use exclamation marks a little more.".into());
    }

    // Lists.
    let list_added = pairs.iter().filter(|(g, f)| !has_list(g) && has_list(f)).count();
    let list_removed = pairs.iter().filter(|(g, f)| has_list(g) && !has_list(f)).count();
    if list_added >= MIN_SAMPLES && list_added > list_removed * 2 {
        rules.push("Use numbered steps for instructions.".into());
    } else if list_removed >= MIN_SAMPLES && list_removed > list_added * 2 {
        rules.push("Write in short sentences instead of lists.".into());
    }

    // Phrases you keep deleting / adding.
    let mut deleted: HashMap<String, usize> = HashMap::new();
    let mut inserted: HashMap<String, usize> = HashMap::new();
    for (g, f) in &pairs {
        let (tg, tf) = (trigrams(g), trigrams(f));
        for t in tg.difference(&tf) {
            *deleted.entry(t.clone()).or_default() += 1;
        }
        for t in tf.difference(&tg) {
            *inserted.entry(t.clone()).or_default() += 1;
        }
    }
    let top = |m: HashMap<String, usize>| {
        let mut v: Vec<(String, usize)> = m.into_iter().filter(|(_, n)| *n >= MIN_SAMPLES).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.into_iter().take(5).map(|(p, _)| format!("\"{p}\"")).collect::<Vec<_>>()
    };
    let avoid = top(deleted);
    if !avoid.is_empty() {
        rules.push(format!("Avoid these phrases (the agent keeps deleting them): {}.", avoid.join(", ")));
    }
    let likes = top(inserted);
    if !likes.is_empty() {
        rules.push(format!("Phrases the agent often adds: {}.", likes.join(", ")));
    }

    (!rules.is_empty()).then(|| rules.iter().map(|r| format!("- {r}")).collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_scores() {
        assert_eq!(similarity("a b c", "a b c"), 1.0);
        assert!(similarity("Hey! Try deleting node_modules and reinstalling.", "Try deleting node_modules.") < SIGNIFICANT);
        assert!(similarity("Hey! Try this fix", "Hey! Try this fix :)") > 0.7);
    }

    #[test]
    fn learns_consistent_edits() {
        let db = db::open_in_memory().unwrap();
        for i in 0..4 {
            let generated = format!("Hey there! I hope you're well. Feel free to reach out anytime. Try step {i} and then restart the server to see the change.");
            let final_text = format!("Try step {i}, then restart the server :)");
            record(&db, Some("technical"), "q", &generated, &final_text, similarity(&generated, &final_text));
        }
        let profile = edit_profile(&db).unwrap();
        assert!(profile.contains("shorter"));
        assert!(profile.contains("Don't open with a greeting"));
        assert!(profile.contains("\"feel free to\""));
        assert!(profile.contains(":)"));
    }

    #[test]
    fn needs_enough_samples() {
        let db = db::open_in_memory().unwrap();
        record(&db, None, "q", "long draft here", "short", 0.2);
        assert!(edit_profile(&db).is_none());
    }
}
