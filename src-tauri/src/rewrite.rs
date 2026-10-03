//! Quick rewrite controls: replace the reply that was just pasted with a new
//! version, in place, without touching anything the user typed.
//!
//! The old reply is located inside the text input via Accessibility
//! (whitespace-insensitive), selected, verified, and pasted over. If it can't
//! be found (e.g. the user already edited it), the new version is only put on
//! the clipboard. Return is never pressed.

use crate::macos::{apps, ax, focus_tracker, keys, pasteboard};
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteAction {
    Shorter,
    Friendlier,
    Professional,
    ExplainMore,
    Regenerate,
}

impl RewriteAction {
    pub const ALL: [RewriteAction; 5] = [
        RewriteAction::Shorter,
        RewriteAction::Friendlier,
        RewriteAction::Professional,
        RewriteAction::ExplainMore,
        RewriteAction::Regenerate,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RewriteAction::Shorter => "Shorter",
            RewriteAction::Friendlier => "Friendlier",
            RewriteAction::Professional => "More professional",
            RewriteAction::ExplainMore => "Explain more",
            RewriteAction::Regenerate => "Regenerate",
        }
    }

    /// Global shortcut while the rewrite bar is visible.
    pub fn shortcut(self) -> &'static str {
        match self {
            RewriteAction::Shorter => "Alt+1",
            RewriteAction::Friendlier => "Alt+2",
            RewriteAction::Professional => "Alt+3",
            RewriteAction::ExplainMore => "Alt+4",
            RewriteAction::Regenerate => "Alt+5",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            RewriteAction::Shorter => "shorter",
            RewriteAction::Friendlier => "friendlier",
            RewriteAction::Professional => "professional",
            RewriteAction::ExplainMore => "explain_more",
            RewriteAction::Regenerate => "regenerate",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
}

pub enum RewriteDelivery {
    Replaced,
    /// Auto-paste off.
    Copied,
    /// Couldn't find the old reply in the input (edited or gone).
    CopiedNotFound,
}

/// Returns how it went, plus the text input the reply now lives in (if known).
pub fn deliver(pid: i32, old_reply: &str, new_reply: &str, auto_paste: bool) -> (RewriteDelivery, Option<ax::Element>) {
    pasteboard::write_string(new_reply);
    if !auto_paste {
        return (RewriteDelivery::Copied, focus_tracker::remembered_input(pid));
    }
    if !apps::ensure_frontmost(pid) {
        return (RewriteDelivery::CopiedNotFound, None);
    }
    for input in candidate_inputs(pid) {
        let Some(value) = ax::value(&input) else { continue };
        let Some((location, length)) = find_range(&value, old_reply) else { continue };
        ax::focus(&input);
        sleep(Duration::from_millis(30));
        if !ax::select_range(&input, location, length) {
            continue;
        }
        sleep(Duration::from_millis(30));
        // Only paste once we've confirmed the old reply is what's selected.
        let selected = input.attr_string("AXSelectedText").unwrap_or_default();
        if same_text(&selected, old_reply) {
            keys::paste();
            return (RewriteDelivery::Replaced, Some(input));
        }
    }
    (RewriteDelivery::CopiedNotFound, focus_tracker::remembered_input(pid))
}

/// Text currently in the reply box (for "Save as example"), if readable.
pub fn current_input_text(pid: i32) -> Option<String> {
    candidate_inputs(pid).into_iter().find_map(|el| ax::value(&el)).filter(|v| !v.trim().is_empty())
}

fn candidate_inputs(pid: i32) -> Vec<ax::Element> {
    let mut out = Vec::new();
    if let Some(el) = ax::focused_element(pid) {
        if ax::is_editable(&el) {
            out.push(el);
        }
    }
    if let Some(el) = focus_tracker::remembered_input(pid) {
        out.push(el);
    }
    out
}

pub fn same_text(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

pub fn contains_text(haystack: &str, needle: &str) -> bool {
    find_range(haystack, needle).is_some()
}

/// Whitespace-collapsed copy of `s`, plus each kept char's UTF-16 span in `s`.
fn normalize(s: &str) -> (Vec<char>, Vec<(usize, usize)>) {
    let mut chars = Vec::with_capacity(s.len());
    let mut spans: Vec<(usize, usize)> = Vec::with_capacity(s.len());
    let mut offset = 0usize;
    let mut in_space = false;
    for c in s.chars() {
        let w = c.len_utf16();
        if c.is_whitespace() || c == '\u{FFFC}' {
            if in_space {
                if let Some(last) = spans.last_mut() {
                    last.1 = offset + w;
                }
            } else {
                chars.push(' ');
                spans.push((offset, offset + w));
                in_space = true;
            }
        } else {
            chars.push(c);
            spans.push((offset, offset + w));
            in_space = false;
        }
        offset += w;
    }
    (chars, spans)
}

/// Last occurrence of `needle` in `haystack`, ignoring whitespace differences
/// (rich editors turn "\n\n" into paragraph breaks). Returns a UTF-16 range.
pub fn find_range(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let (h, spans) = normalize(haystack);
    let (n, _) = normalize(needle.trim());
    if n.is_empty() || n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len()).rev().find(|&i| h[i..i + n.len()] == n[..]).map(|i| {
        let start = spans[i].0;
        let end = spans[i + n.len() - 1].1;
        (start, end - start)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_reply_despite_whitespace_changes() {
        let field = "Hi Sam!\nTry this:\n\n1. Delete node_modules\n2. Run npm install";
        let reply = "Try this:\n\n\n1. Delete node_modules 2. Run npm install";
        let (start, len) = find_range(field, reply).unwrap();
        let utf16: Vec<u16> = field.encode_utf16().collect();
        let picked = String::from_utf16(&utf16[start..start + len]).unwrap();
        assert!(picked.starts_with("Try this:"));
        assert!(picked.ends_with("npm install"));
    }

    #[test]
    fn utf16_offsets_with_emoji() {
        let field = "😀 Hello there :)";
        let (start, len) = find_range(field, "Hello there :)").unwrap();
        assert_eq!(start, 3); // emoji is 2 UTF-16 units + space
        assert_eq!(len, "Hello there :)".len());
    }

    #[test]
    fn edited_reply_is_not_found() {
        assert!(find_range("Hi! I changed this reply", "Hi! Original reply").is_none());
    }
}
