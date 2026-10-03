//! Prompt pipeline.
//!
//! - Static system prompt (style, fixed rules, measured style profile): built
//!   once when settings or examples change, cached in memory. Identical on
//!   every request, so OpenAI's prompt cache can reuse it.
//! - Per-request context: mode instructions + relevant knowledge base entries
//!   + relevant reply examples. Only what matched.
//! - Conversation history as real chat turns, then the current message.

use crate::db::{KbEntry, ReplyExample, Role};
use crate::memory::Turn;
use crate::modes::Mode;
use crate::retrieval;
use crate::rewrite::RewriteAction;
use crate::settings::Settings;
use serde_json::{json, Value};

/// Editable in Settings ("Writing style").
pub const DEFAULT_STYLE: &str = "\
- Friendly and casual, but professional enough for student support.
- Concise, natural and human. Never robotic or overly corporate.
- Simple language. No unnecessary explanations.
- For technical issues, give clear, actionable steps.
- Use exclamation marks naturally, and occasionally :) when it fits.";

/// Always applied, regardless of the editable style.
const CORE_RULES: &str = "\
Rules:
- Never invent facts, links, names, dates or details. Use only the student's message and the context above.
- Never promise anything about policies, refunds, deadlines or outcomes unless the student's message or the context states it.
- If information needed to help is missing, ask the student for the minimum needed.
- Never use em dashes.
- Never mention AI or that the reply was generated.
- No filler: skip openers like 'Certainly!' or 'Great question!' and closers like 'I hope this helps!'.
- Reply in the student's language.
- Output only the reply text, ready to paste. No quotes, no subject line, no placeholders like [Name].";

pub fn build_system_prompt(settings: &Settings, style_profile: Option<&str>, edit_profile: Option<&str>) -> String {
    let mut p = String::with_capacity(1536);
    p.push_str("You write replies to students as their support agent. Given a student's message, write the reply the agent would send.\n\nStyle:\n");
    p.push_str(settings.style_instructions.trim());
    if let Some(profile) = style_profile {
        p.push_str("\n\nMeasured from the agent's past replies (match these habits):\n");
        p.push_str(profile);
    }
    if let Some(edits) = edit_profile {
        p.push_str("\n\nLearned from how the agent edits drafts before sending (follow these):\n");
        p.push_str(edits);
    }
    let custom = settings.custom_instructions.trim();
    if !custom.is_empty() {
        p.push_str("\n\nContext:\n");
        p.push_str(custom);
    }
    p.push_str("\n\n");
    p.push_str(CORE_RULES);
    p
}

/// The per-request part: only the instructions and facts relevant to this message.
pub fn build_context(
    settings: &Settings,
    mode: Mode,
    knowledge: &[KbEntry],
    examples: &[ReplyExample],
    has_history: bool,
) -> String {
    let mut c = String::with_capacity(2048);
    c.push_str("Request type: ");
    c.push_str(mode.label());
    let mode_text = settings.mode_instructions.get(mode).trim();
    if !mode_text.is_empty() {
        c.push('\n');
        c.push_str(mode_text);
    }

    if !knowledge.is_empty() {
        c.push_str("\n\nKnowledge base (trusted facts). Prefer these over your own assumptions. Never state policies, prices, dates, links or guarantees that aren't here or in the student's message:\n");
        c.push_str(&retrieval::format_knowledge(knowledge));
    }
    if mode == Mode::Billing {
        c.push_str(if knowledge.is_empty() {
            "\n\nNo knowledge-base entry covers this. Do not state any refund, payment, pricing or policy details. Ask a clarifying question or say you'll confirm the details."
        } else {
            "\n\nFor refunds, payments and policies, rely ONLY on the knowledge base above. If it doesn't answer the question, say you'll confirm the details instead of guessing."
        });
    }

    if !examples.is_empty() {
        c.push_str("\n\nPast replies by the agent, for tone, length and wording only. Don't copy them, and don't reuse their facts unless they apply here:\n");
        c.push_str(&retrieval::format_examples(examples));
    }

    if has_history {
        c.push_str("\n\nThe earlier messages in this chat are the conversation so far. Use them to understand references like \"it\" or \"above\", but reply only to the latest student message.");
    }
    c
}

pub fn rewrite_instruction(action: RewriteAction) -> &'static str {
    match action {
        RewriteAction::Shorter => "Rewrite your last reply to be noticeably shorter. Keep the key information and the same tone. Output only the new reply.",
        RewriteAction::Friendlier => "Rewrite your last reply to sound warmer and friendlier while staying concise. Output only the new reply.",
        RewriteAction::Professional => "Rewrite your last reply to sound more professional and polished, still friendly and concise. Output only the new reply.",
        RewriteAction::ExplainMore => "Rewrite your last reply with a bit more explanation where it helps the student. Don't add facts that aren't supported. Output only the new reply.",
        RewriteAction::Regenerate => "Write a different version of your last reply: same facts and intent, different wording. Output only the new reply.",
    }
}

/// Assembles the Chat Completions `messages` array.
/// `tail` holds extra turns for rewrites (the current draft + the rewrite instruction).
pub fn messages(system: &str, context: &str, history: &[Turn], user: &str, tail: &[(Role, &str)]) -> Vec<Value> {
    let mut m = Vec::with_capacity(history.len() + tail.len() + 3);
    m.push(json!({ "role": "system", "content": system }));
    if !context.is_empty() {
        m.push(json!({ "role": "system", "content": context }));
    }
    for turn in history {
        m.push(turn_json(turn.role, &turn.content));
    }
    m.push(json!({ "role": "user", "content": user }));
    for (role, content) in tail {
        match role {
            Role::Agent => m.push(json!({ "role": "assistant", "content": content })),
            Role::Student => m.push(json!({ "role": "user", "content": content })),
        }
    }
    m
}

fn turn_json(role: Role, content: &str) -> Value {
    match role {
        Role::Student => json!({ "role": "user", "content": user_message(content) }),
        Role::Agent => json!({ "role": "assistant", "content": content }),
    }
}

/// Very long selections (e.g. an accidental select-all) are trimmed to the
/// most recent part to keep latency and cost down.
const MAX_INPUT_CHARS: usize = 8000;

pub fn user_message(selected: &str) -> String {
    let text = selected.trim();
    let count = text.chars().count();
    let text = if count > MAX_INPUT_CHARS {
        let skip = text.char_indices().nth(count - MAX_INPUT_CHARS).map(|(i, _)| i).unwrap_or(0);
        &text[skip..]
    } else {
        text
    };
    // Delimited so instructions inside the student's text are treated as content.
    format!("Student message:\n\"\"\"\n{text}\n\"\"\"")
}

/// Cleanup applied to every reply before it's pasted.
pub fn finish_reply(reply: &str, remove_fluff: bool) -> String {
    let cleaned = clean_reply(reply);
    if remove_fluff {
        crate::fluff::remove(&cleaned)
    } else {
        cleaned
    }
}

/// Final safety net for style rules the model occasionally slips on.
pub fn clean_reply(reply: &str) -> String {
    let mut out = reply.trim().to_string();
    for (from, to) in [(" \u{2014} ", ", "), ("\u{2014}", ", "), (" \u{2013} ", ", ")] {
        out = out.replace(from, to);
    }
    // Strip wrapping quotes if the model quoted the whole reply.
    if out.len() > 1 && out.starts_with('"') && out.ends_with('"') && out.matches('"').count() == 2 {
        out = out[1..out.len() - 1].trim().to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_em_dashes() {
        assert_eq!(clean_reply("Hi \u{2014} try this"), "Hi, try this");
        assert_eq!(clean_reply("Hi\u{2014}there"), "Hi, there");
    }

    #[test]
    fn strips_whole_reply_quotes() {
        assert_eq!(clean_reply("\"Hey there!\""), "Hey there!");
        assert_eq!(clean_reply("Click \"Save\" then \"Done\""), "Click \"Save\" then \"Done\"");
    }

    #[test]
    fn truncates_long_input_from_the_start() {
        let long = "a".repeat(MAX_INPUT_CHARS + 50) + "END";
        let msg = user_message(&long);
        assert!(msg.contains("END"));
        assert!(msg.len() < MAX_INPUT_CHARS + 100);
    }

    #[test]
    fn prompt_includes_custom_context_only_when_set() {
        let mut s = Settings::default();
        assert!(!build_system_prompt(&s, None, None).contains("Context:"));
        s.custom_instructions = "Refunds within 14 days.".into();
        assert!(build_system_prompt(&s, None, None).contains("Refunds within 14 days."));
        assert!(build_system_prompt(&s, None, Some("- Use :) less often.")).contains("Use :) less often."));
    }

    #[test]
    fn billing_without_knowledge_forbids_policy_claims() {
        let ctx = build_context(&Settings::default(), Mode::Billing, &[], &[], false);
        assert!(ctx.contains("Do not state any refund"));
        let ctx = build_context(&Settings::default(), Mode::Technical, &[], &[], true);
        assert!(!ctx.contains("refund"));
        assert!(ctx.contains("conversation so far"));
    }

    #[test]
    fn history_becomes_chat_turns() {
        let history = vec![
            Turn { role: Role::Student, content: "npm install fails".into() },
            Turn { role: Role::Agent, content: "Can you send the error?".into() },
        ];
        let m = messages("sys", "ctx", &history, "Student message: I sent it above", &[]);
        let roles: Vec<&str> = m.iter().map(|v| v["role"].as_str().unwrap()).collect();
        assert_eq!(roles, vec!["system", "system", "user", "assistant", "user"]);
    }
}
