//! Bulk import parsers for reply examples and knowledge-base entries.
//!
//! Reply examples:
//!   - JSON array: [{"student": "...", "reply": "...", "category": "..."}]
//!     (also accepts studentMessage/student_message/question and me/answer/response)
//!   - Text blocks separated by a line of `---`:
//!       Student: ...
//!       Me: ...            (or Reply: / Agent:)
//!       Category: ...      (optional)
//!
//! Knowledge base:
//!   - JSON array: [{"title": "...", "content": "...", "category": "...", "tags": "a, b" | ["a","b"], "enabled": true}]
//!   - Markdown: each `#`/`##` heading starts an entry; optional `Category:` and
//!     `Tags:` lines right under the heading; the rest is the content.

use crate::db::{KbEntry, ReplyExample};
use serde_json::Value;

fn pick(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| v.get(*k))
        .map(|x| match x {
            Value::String(s) => s.trim().to_string(),
            Value::Array(items) => items.iter().filter_map(|i| i.as_str()).collect::<Vec<_>>().join(", "),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

pub fn parse_examples(input: &str) -> Result<Vec<ReplyExample>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Nothing to import.".into());
    }
    let out: Vec<ReplyExample> = if trimmed.starts_with('[') {
        let items: Vec<Value> = serde_json::from_str(trimmed).map_err(|e| format!("Invalid JSON: {e}"))?;
        items
            .iter()
            .map(|v| ReplyExample {
                id: None,
                student_message: pick(v, &["student", "studentMessage", "student_message", "question", "message"]),
                reply: pick(v, &["reply", "me", "answer", "response", "agent"]),
                category: pick(v, &["category", "mode"]),
            })
            .collect()
    } else {
        blocks(trimmed)
            .into_iter()
            .map(|block| {
                let fields = labeled_fields(&block, &["student", "me", "reply", "agent", "category"]);
                let get = |names: &[&str]| {
                    names.iter().find_map(|n| fields.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone())).unwrap_or_default()
                };
                ReplyExample {
                    id: None,
                    student_message: get(&["student"]),
                    reply: get(&["me", "reply", "agent"]),
                    category: get(&["category"]),
                }
            })
            .collect()
    };
    let valid: Vec<ReplyExample> =
        out.into_iter().filter(|e| !e.student_message.is_empty() && !e.reply.is_empty()).collect();
    if valid.is_empty() {
        return Err("No examples found. Each needs a student message and a reply.".into());
    }
    Ok(valid)
}

pub fn parse_kb(input: &str) -> Result<Vec<KbEntry>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Nothing to import.".into());
    }
    let out: Vec<KbEntry> = if trimmed.starts_with('[') {
        let items: Vec<Value> = serde_json::from_str(trimmed).map_err(|e| format!("Invalid JSON: {e}"))?;
        items
            .iter()
            .map(|v| KbEntry {
                id: None,
                title: pick(v, &["title", "name", "question"]),
                content: pick(v, &["content", "body", "text", "answer"]),
                category: pick(v, &["category"]),
                tags: pick(v, &["tags"]),
                enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            })
            .collect()
    } else {
        markdown_entries(trimmed)
    };
    let valid: Vec<KbEntry> = out.into_iter().filter(|e| !e.title.is_empty() && !e.content.is_empty()).collect();
    if valid.is_empty() {
        return Err("No entries found. Use a JSON array or Markdown headings.".into());
    }
    Ok(valid)
}

fn blocks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.trim() == "---" {
            if !current.trim().is_empty() {
                out.push(std::mem::take(&mut current));
            }
            current.clear();
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// "Label: value" lines; values continue on following lines until the next label.
fn labeled_fields(block: &str, labels: &[&str]) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in block.lines() {
        let label = line.split_once(':').and_then(|(k, v)| {
            let k = k.trim().to_lowercase();
            labels.contains(&k.as_str()).then(|| (k, v.trim().to_string()))
        });
        match label {
            Some((k, v)) => fields.push((k, v)),
            None => {
                if let Some((_, v)) = fields.last_mut() {
                    if !v.is_empty() {
                        v.push('\n');
                    }
                    v.push_str(line);
                }
            }
        }
    }
    for (_, v) in fields.iter_mut() {
        *v = v.trim().to_string();
    }
    fields
}

fn markdown_entries(text: &str) -> Vec<KbEntry> {
    let mut out = Vec::new();
    let mut current: Option<KbEntry> = None;
    let mut in_header = false;
    for line in text.lines() {
        let t = line.trim();
        if let Some(title) = t.strip_prefix("## ").or_else(|| t.strip_prefix("# ")) {
            if let Some(e) = current.take() {
                out.push(e);
            }
            current = Some(KbEntry {
                id: None,
                title: title.trim().to_string(),
                content: String::new(),
                category: String::new(),
                tags: String::new(),
                enabled: true,
            });
            in_header = true;
            continue;
        }
        let Some(e) = current.as_mut() else { continue };
        if in_header {
            if let Some(v) = t.strip_prefix("Category:") {
                e.category = v.trim().to_string();
                continue;
            }
            if let Some(v) = t.strip_prefix("Tags:") {
                e.tags = v.trim().to_string();
                continue;
            }
            if t.is_empty() {
                continue;
            }
            in_header = false;
        }
        e.content.push_str(line);
        e.content.push('\n');
    }
    if let Some(e) = current {
        out.push(e);
    }
    for e in out.iter_mut() {
        e.content = e.content.trim().to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_examples_from_text_blocks() {
        let input = "Student: My npm install fails\nwith EACCES\nMe: Hey! Try this:\n1. sudo chown\nCategory: technical\n---\nStudent: refund?\nReply: Let me check :)";
        let ex = parse_examples(input).unwrap();
        assert_eq!(ex.len(), 2);
        assert_eq!(ex[0].student_message, "My npm install fails\nwith EACCES");
        assert_eq!(ex[0].reply, "Hey! Try this:\n1. sudo chown");
        assert_eq!(ex[0].category, "technical");
        assert_eq!(ex[1].reply, "Let me check :)");
    }

    #[test]
    fn imports_examples_from_json() {
        let ex = parse_examples(r#"[{"question":"Hi","answer":"Hey!"},{"student":"x"}]"#).unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].reply, "Hey!");
    }

    #[test]
    fn imports_kb_from_markdown_and_json() {
        let md = "# Refund policy\nCategory: billing\nTags: refund, money back\n\nFull refund within 14 days.\n\n## Course length\n12 weeks.";
        let kb = parse_kb(md).unwrap();
        assert_eq!(kb.len(), 2);
        assert_eq!(kb[0].category, "billing");
        assert_eq!(kb[0].tags, "refund, money back");
        assert_eq!(kb[0].content, "Full refund within 14 days.");
        let kb = parse_kb(r#"[{"title":"A","content":"B","tags":["x","y"],"enabled":false}]"#).unwrap();
        assert_eq!(kb[0].tags, "x, y");
        assert!(!kb[0].enabled);
    }
}
