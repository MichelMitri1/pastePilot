//! Smart modes: a local keyword classifier (microseconds, no AI call) plus
//! per-mode instructions that are editable in Settings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Technical,
    Mentoring,
    Billing,
    Career,
    General,
}

impl Mode {
    pub const ALL: [Mode; 5] = [Mode::Technical, Mode::Mentoring, Mode::Billing, Mode::Career, Mode::General];

    pub fn id(self) -> &'static str {
        match self {
            Mode::Technical => "technical",
            Mode::Mentoring => "mentoring",
            Mode::Billing => "billing",
            Mode::Career => "career",
            Mode::General => "general",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Technical => "Technical",
            Mode::Mentoring => "Mentoring",
            Mode::Billing => "Billing",
            Mode::Career => "Career",
            Mode::General => "General",
        }
    }

    pub fn from_id(id: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.id().eq_ignore_ascii_case(id.trim()))
    }

    pub fn index(self) -> usize {
        Mode::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    /// Extra search terms so e.g. "can I get my money back" still finds a "Refund policy" entry.
    pub fn search_expansion(self) -> &'static [&'static str] {
        match self {
            Mode::Billing => &["refund", "payment", "billing", "subscription", "cancel", "invoice", "charge", "price", "plan", "policy"],
            Mode::Career => &["career", "job", "guarantee", "interview", "resume", "portfolio"],
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ModeInstructions {
    pub technical: String,
    pub mentoring: String,
    pub billing: String,
    pub career: String,
    pub general: String,
}

impl Default for ModeInstructions {
    fn default() -> Self {
        Self {
            technical: "- Diagnose the likely cause from what the student shared.\n- Give clear, numbered, actionable steps.\n- Keep explanations simple. If you need an error message or details, ask for exactly that.".into(),
            mentoring: "- Guide the student toward the answer instead of doing everything for them, unless that's clearly more helpful.\n- Be encouraging and focus on learning.\n- Suggest a concrete next step.".into(),
            billing: "- Be careful: only state refund, payment, pricing or subscription details that appear in the knowledge base or the student's message.\n- If the answer isn't covered, don't guess. Ask a clarifying question or say you'll confirm the details.\n- Stay empathetic and calm.".into(),
            career: "- Professional but supportive.\n- Give practical, specific advice.\n- Only mention programs or guarantees that appear in the knowledge base.".into(),
            general: "- Concise, helpful and friendly.".into(),
        }
    }
}

impl ModeInstructions {
    pub fn get(&self, mode: Mode) -> &str {
        match mode {
            Mode::Technical => &self.technical,
            Mode::Mentoring => &self.mentoring,
            Mode::Billing => &self.billing,
            Mode::Career => &self.career,
            Mode::General => &self.general,
        }
    }
}

// (keyword or phrase, weight). Matched on word boundaries, lowercase.
const TECHNICAL: &[(&str, u32)] = &[
    ("error", 3), ("bug", 3), ("crash", 3), ("exception", 3), ("traceback", 3), ("stack trace", 3), ("npm", 3),
    ("yarn", 3), ("pip", 3), ("install", 2), ("installed", 2), ("installing", 2), ("not working", 2),
    ("doesn't work", 2), ("doesnt work", 2), ("isn't working", 2), ("broken", 2), ("code", 2), ("function", 2),
    ("terminal", 3), ("command", 2), ("compile", 3), ("build", 2), ("deploy", 3), ("localhost", 3), ("port", 1),
    ("git", 3), ("github", 2), ("python", 2), ("javascript", 2), ("react", 2), ("node", 2), ("api", 2),
    ("undefined", 3), ("null", 2), ("syntax", 3), ("console", 2), ("module", 2), ("package", 1), ("import", 1),
    ("css", 2), ("html", 2), ("sql", 2), ("database", 2), ("server", 2), ("404", 2), ("500", 2), ("debug", 3),
    ("vscode", 3), ("vs code", 3), ("browser", 1), ("login", 1), ("log in", 1), ("password", 1), ("can't access", 1),
    ("video won't", 2), ("won't load", 2), ("loading", 1), ("upload", 1),
];
const BILLING: &[(&str, u32)] = &[
    ("refund", 4), ("refunds", 4), ("money back", 4), ("charge", 3), ("charged", 4), ("payment", 3), ("pay", 2),
    ("paid", 2), ("invoice", 4), ("receipt", 3), ("subscription", 3), ("subscribe", 2), ("cancel", 3),
    ("cancellation", 3), ("billing", 4), ("bill", 2), ("price", 3), ("pricing", 3), ("discount", 3), ("coupon", 3),
    ("credit card", 3), ("card", 1), ("renew", 3), ("renewal", 3), ("trial", 2), ("plan", 1), ("upgrade", 1),
    ("downgrade", 2), ("installment", 3), ("financing", 3), ("tuition", 3), ("fee", 3), ("fees", 3),
];
const CAREER: &[(&str, u32)] = &[
    ("job", 3), ("jobs", 3), ("career", 4), ("interview", 4), ("interviews", 4), ("resume", 4), ("cv", 3),
    ("portfolio", 3), ("linkedin", 4), ("hiring", 3), ("salary", 4), ("offer", 2), ("internship", 4),
    ("recruiter", 4), ("job guarantee", 5), ("apply", 2), ("applying", 2), ("application", 1), ("employer", 3),
    ("cover letter", 4), ("junior", 2), ("position", 2),
];
const MENTORING: &[(&str, u32)] = &[
    ("stuck", 2), ("motivation", 3), ("motivated", 3), ("learn", 2), ("learning", 2), ("understand", 2),
    ("how should i", 3), ("best way", 3), ("advice", 2), ("struggling", 3), ("overwhelmed", 3), ("confused", 2),
    ("concept", 3), ("roadmap", 3), ("study", 2), ("practice", 2), ("feedback", 2), ("progress", 2),
    ("what should i", 3), ("give up", 3), ("difficult", 2), ("hard to", 2), ("explain", 1), ("next step", 2),
    ("project idea", 3), ("review my", 3),
];

const MIN_SCORE: u32 = 2;

/// Classifies a student message. Ambiguous follow-ups ("I sent it above")
/// inherit the conversation's previous mode.
pub fn classify(text: &str, previous: Option<Mode>) -> Mode {
    let lower = text.to_lowercase();
    let mut scores = [
        (Mode::Billing, score(&lower, BILLING)),
        (Mode::Technical, score(&lower, TECHNICAL) + code_signals(text)),
        (Mode::Career, score(&lower, CAREER)),
        (Mode::Mentoring, score(&lower, MENTORING)),
    ];
    // Stable sort keeps the priority order above on ties (billing is the safety-critical one).
    scores.sort_by(|a, b| b.1.cmp(&a.1));
    let (best, best_score) = scores[0];
    if best_score >= MIN_SCORE {
        best
    } else {
        previous.unwrap_or(Mode::General)
    }
}

fn score(lower: &str, keywords: &[(&str, u32)]) -> u32 {
    keywords.iter().filter(|(k, _)| contains_word(lower, k)).map(|(_, w)| w).sum()
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let i = start + pos;
        let end = i + needle.len();
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = i + 1;
        while !haystack.is_char_boundary(start) {
            start += 1;
        }
    }
    false
}

/// Pasted code or stack traces are a strong technical signal.
fn code_signals(text: &str) -> u32 {
    let mut s = 0;
    if text.contains("```") || text.contains("=>") || text.contains("();") || text.contains("{\n") {
        s += 3;
    }
    if text.lines().any(|l| l.trim_start().starts_with("at ") && l.contains('(')) {
        s += 3;
    }
    if text.contains("Error:") || text.contains("ERR!") || text.contains("Traceback") {
        s += 3;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_requests() {
        assert_eq!(classify("My npm install isn't working", None), Mode::Technical);
        assert_eq!(classify("Can I get a refund? I was charged twice", None), Mode::Billing);
        assert_eq!(classify("Any tips for my resume before the interview?", None), Mode::Career);
        assert_eq!(classify("I feel stuck and overwhelmed, how should I study?", None), Mode::Mentoring);
        assert_eq!(classify("Thanks so much!", None), Mode::General);
    }

    #[test]
    fn follow_ups_inherit_previous_mode() {
        assert_eq!(classify("I sent it above.", Some(Mode::Technical)), Mode::Technical);
    }

    #[test]
    fn word_boundaries() {
        assert!(!contains_word("paypal", "pay"));
        assert!(contains_word("i want to pay.", "pay"));
        assert!(!contains_word("capital", "api"));
    }
}
