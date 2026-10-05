//! User settings, stored as JSON in the app config dir
//! (~/Library/Application Support/com.pastepilot.app/settings.json).
//! The API key is NOT stored here; see keychain.rs.

use crate::modes::ModeInstructions;
use crate::prompt::DEFAULT_STYLE;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_MODEL: &str = "gpt-4.1-mini";
pub const DEFAULT_SHORTCUT: &str = "Alt+R";
pub const DEFAULT_NEW_CONVERSATION_SHORTCUT: &str = "Alt+Shift+R";
pub const DEFAULT_DEBUG_SHORTCUT: &str = "Alt+G";
pub const DEFAULT_VOICE_SHORTCUT: &str = "Alt+V";
pub const DEFAULT_REVIEW_SHORTCUT: &str = "Alt+A";
const LEGACY_REVIEW_SHORTCUT: &str = "Alt+Shift+A";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub model: String,
    pub shortcut: String,
    pub style_instructions: String,
    pub custom_instructions: String,
    pub auto_paste: bool,
    pub launch_at_login: bool,
    pub show_hud: bool,

    // Conversation memory
    pub memory_enabled: bool,
    /// Group by app + page + window title. Off = one manual session.
    pub memory_auto_detect: bool,
    pub memory_max_messages: u32,
    pub memory_max_chars: u32,
    /// A conversation idle longer than this starts fresh.
    pub memory_expire_hours: u32,

    // Smart modes
    /// "auto" or a mode id ("technical", "billing", ...).
    pub mode_override: String,
    pub mode_instructions: ModeInstructions,

    // Retrieval
    pub examples_enabled: bool,
    pub max_examples: u32,
    pub kb_enabled: bool,
    pub max_kb_entries: u32,

    // Rewrite controls and shortcuts
    pub rewrite_bar: bool,
    /// ⌥1–⌥5 while the rewrite bar is visible.
    pub rewrite_shortcuts: bool,
    /// Starts a fresh conversation, then generates. Empty = none.
    pub new_conversation_shortcut: String,

    // GitHub Debug Mode
    /// Opens GitHub Debug Mode. Empty = none.
    pub debug_shortcut: String,
    /// Model for code analysis (empty = same as `model`).
    pub debug_model: String,
    /// Always compare recent commits (otherwise only when the student says something broke).
    pub debug_compare_commits: bool,
    /// ⌥G with a repo link + problem in the selection: diagnose and paste without opening the window.
    pub debug_one_click: bool,
    /// Put the corrected code in one-click replies when there is one.
    pub debug_include_snippet: bool,

    // Reply polish and learning
    /// Strip generic AI phrasing ("Certainly!", "It appears that") before pasting.
    pub remove_fluff: bool,
    /// Learn style rules from how much you edit drafts before sending.
    pub feedback_learning: bool,
    /// Save heavily edited replies as reply examples.
    pub feedback_save_examples: bool,

    // Assignment Review
    /// Opens Assignment Review. Empty = none.
    pub review_shortcut: String,

    // Clipboard history
    pub clipboard_history: bool,
    /// Also record text you copy yourself (off by default; password-manager copies are always skipped).
    pub clipboard_watch: bool,
    pub clipboard_max_items: u32,
    pub clipboard_max_days: u32,

    // Voice commands
    pub voice_enabled: bool,
    /// Hold to talk. Empty = none.
    pub voice_shortcut: String,

    // Analytics
    pub minutes_per_reply: f64,
    pub minutes_per_debug: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.into(),
            shortcut: DEFAULT_SHORTCUT.into(),
            style_instructions: DEFAULT_STYLE.into(),
            custom_instructions: String::new(),
            auto_paste: true,
            launch_at_login: false,
            show_hud: true,
            memory_enabled: true,
            memory_auto_detect: true,
            memory_max_messages: 6,
            memory_max_chars: 3000,
            memory_expire_hours: 12,
            mode_override: "auto".into(),
            mode_instructions: ModeInstructions::default(),
            examples_enabled: true,
            max_examples: 3,
            kb_enabled: true,
            max_kb_entries: 3,
            rewrite_bar: true,
            rewrite_shortcuts: true,
            new_conversation_shortcut: DEFAULT_NEW_CONVERSATION_SHORTCUT.into(),
            debug_shortcut: DEFAULT_DEBUG_SHORTCUT.into(),
            debug_model: DEFAULT_MODEL.into(),
            debug_compare_commits: false,
            debug_one_click: true,
            debug_include_snippet: false,
            remove_fluff: true,
            feedback_learning: true,
            feedback_save_examples: true,
            review_shortcut: DEFAULT_REVIEW_SHORTCUT.into(),
            clipboard_history: true,
            clipboard_watch: false,
            clipboard_max_items: 200,
            clipboard_max_days: 14,
            voice_enabled: false,
            voice_shortcut: DEFAULT_VOICE_SHORTCUT.into(),
            minutes_per_reply: 2.0,
            minutes_per_debug: 10.0,
        }
    }
}

pub fn load(path: &Path) -> Settings {
    let mut settings: Settings = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    // Move existing installations from the former default to the replacement.
    if settings.review_shortcut == LEGACY_REVIEW_SHORTCUT {
        settings.review_shortcut = DEFAULT_REVIEW_SHORTCUT.into();
    }

    settings
}

pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(settings).expect("settings serialize");
    // Write-then-rename so a crash never leaves a half-written file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)
}
