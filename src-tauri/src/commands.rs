//! Commands invoked by the Settings window (see src/api.ts).

use crate::db::{self, ConversationInfo, KbEntry, ReplyExample};
use crate::platform::ax;
use crate::modes::ModeInstructions;
use crate::prompt::DEFAULT_STYLE;
use crate::settings::Settings;
use crate::state::AppState;
use crate::casebook::{self, Fix, IssueRecord};
use crate::cliphistory::{self, ClipItem};
use crate::{analytics, case, feedback, flow, import, shortcut, tray};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

type CmdResult<T> = Result<T, String>;

fn db_err(e: rusqlite::Error) -> String {
    format!("Database error: {e}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    has_api_key: bool,
    accessibility_trusted: bool,
    default_style: &'static str,
    default_mode_instructions: ModeInstructions,
    db_error: Option<String>,
}

// ----- General / AI / style -----------------------------------------------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> CmdResult<()> {
    let mut settings = settings;
    settings.model = settings.model.trim().to_owned();
    if settings.model.is_empty() {
        return Err("Choose a model.".into());
    }
    settings.memory_max_messages = settings.memory_max_messages.min(50);
    settings.memory_max_chars = settings.memory_max_chars.clamp(200, 20_000);
    settings.memory_expire_hours = settings.memory_expire_hours.clamp(1, 24 * 30);
    settings.max_examples = settings.max_examples.min(8);
    settings.max_kb_entries = settings.max_kb_entries.min(8);

    let old = state.settings();
    shortcut::apply(&app, &old, &settings)?;

    let autostart = app.autolaunch();
    let currently = autostart.is_enabled().unwrap_or(false);
    if settings.launch_at_login != currently {
        let result = if settings.launch_at_login { autostart.enable() } else { autostart.disable() };
        result.map_err(|e| format!("Couldn't change launch at login: {e}"))?;
    }

    state.replace_settings(settings.clone())?;
    tray::sync(&app, &settings);
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Status {
    Status {
        has_api_key: state.api_key().is_some(),
        accessibility_trusted: ax::is_trusted(),
        default_style: DEFAULT_STYLE,
        default_mode_instructions: ModeInstructions::default(),
        db_error: state.db_error.clone(),
    }
}

#[tauri::command]
pub fn set_api_key(state: State<AppState>, key: String) -> CmdResult<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Paste your OpenAI API key.".into());
    }
    state.set_api_key(key)
}

#[tauri::command]
pub fn clear_api_key(state: State<AppState>) {
    state.clear_api_key();
}

#[tauri::command]
pub fn request_accessibility() -> bool {
    ax::prompt_for_trust()
}

#[tauri::command]
pub fn open_accessibility_settings() {
    // Windows has no Accessibility permission to grant.
    #[cfg(target_os = "macos")]
    let _ = crate::platform::open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
}

// ----- Reply examples -----------------------------------------------------

#[tauri::command]
pub fn list_examples(state: State<AppState>) -> CmdResult<Vec<ReplyExample>> {
    db::list_examples(&state.db()).map_err(db_err)
}

#[tauri::command]
pub fn save_example(state: State<AppState>, example: ReplyExample) -> CmdResult<i64> {
    if example.student_message.trim().is_empty() || example.reply.trim().is_empty() {
        return Err("Both the student message and your reply are needed.".into());
    }
    let id = db::save_example(&state.db(), &example).map_err(db_err)?;
    state.refresh_style_profile();
    Ok(id)
}

#[tauri::command]
pub fn delete_example(state: State<AppState>, id: i64) -> CmdResult<()> {
    db::delete_example(&state.db(), id).map_err(db_err)?;
    state.refresh_style_profile();
    Ok(())
}

#[tauri::command]
pub fn clear_examples(state: State<AppState>) -> CmdResult<()> {
    db::clear_examples(&state.db()).map_err(db_err)?;
    state.refresh_style_profile();
    Ok(())
}

#[tauri::command]
pub fn import_examples(state: State<AppState>, text: String) -> CmdResult<usize> {
    let items = import::parse_examples(&text)?;
    {
        let mut db = state.db();
        let tx = db.transaction().map_err(db_err)?;
        for item in &items {
            db::save_example(&tx, item).map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
    }
    state.refresh_style_profile();
    Ok(items.len())
}

// ----- Knowledge base -----------------------------------------------------

#[tauri::command]
pub fn list_kb(state: State<AppState>) -> CmdResult<Vec<KbEntry>> {
    db::list_kb(&state.db()).map_err(db_err)
}

#[tauri::command]
pub fn save_kb(state: State<AppState>, entry: KbEntry) -> CmdResult<i64> {
    if entry.title.trim().is_empty() || entry.content.trim().is_empty() {
        return Err("Title and content are needed.".into());
    }
    db::save_kb(&state.db(), &entry).map_err(db_err)
}

#[tauri::command]
pub fn set_kb_enabled(state: State<AppState>, id: i64, enabled: bool) -> CmdResult<()> {
    db::set_kb_enabled(&state.db(), id, enabled).map_err(db_err)
}

#[tauri::command]
pub fn delete_kb(state: State<AppState>, id: i64) -> CmdResult<()> {
    db::delete_kb(&state.db(), id).map_err(db_err)
}

#[tauri::command]
pub fn clear_kb(state: State<AppState>) -> CmdResult<()> {
    db::clear_kb(&state.db()).map_err(db_err)
}

#[tauri::command]
pub fn import_kb(state: State<AppState>, text: String) -> CmdResult<usize> {
    let items = import::parse_kb(&text)?;
    let mut db = state.db();
    let tx = db.transaction().map_err(db_err)?;
    for item in &items {
        db::save_kb(&tx, item).map_err(db_err)?;
    }
    tx.commit().map_err(db_err)?;
    Ok(items.len())
}

// ----- Conversation memory ------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatus {
    conversations: i64,
    messages: i64,
    current: Option<ConversationInfo>,
}

#[tauri::command]
pub fn get_memory(state: State<AppState>) -> CmdResult<MemoryStatus> {
    let db = state.db();
    let (conversations, messages) = db::conversation_counts(&db).map_err(db_err)?;
    let current = match state.current_conversation() {
        Some(id) => db::conversation_info(&db, id).map_err(db_err)?,
        None => None,
    };
    Ok(MemoryStatus { conversations, messages, current })
}

#[tauri::command]
pub fn new_conversation(app: AppHandle) {
    flow::new_conversation(&app);
}

#[tauri::command]
pub fn clear_current_conversation(app: AppHandle) {
    flow::clear_current_conversation(&app);
}

#[tauri::command]
pub fn clear_all_conversations(state: State<AppState>) -> CmdResult<()> {
    db::clear_conversations(&state.db()).map_err(db_err)?;
    state.set_current_conversation(None);
    state.set_last_reply(None);
    Ok(())
}

// ----- Fix library ----------------------------------------------------------

#[tauri::command]
pub fn list_fixes(state: State<AppState>) -> CmdResult<Vec<Fix>> {
    casebook::list_fixes(&state.db()).map_err(db_err)
}

#[tauri::command]
pub fn save_fix(state: State<AppState>, fix: Fix) -> CmdResult<i64> {
    if fix.title.trim().is_empty() || fix.solution.trim().is_empty() {
        return Err("A fix needs a title and a solution.".into());
    }
    casebook::save_fix(&state.db(), &fix).map_err(db_err)
}

#[tauri::command]
pub fn delete_fix(state: State<AppState>, id: i64) -> CmdResult<()> {
    casebook::delete_fix(&state.db(), id).map_err(db_err)
}

#[tauri::command]
pub fn clear_fixes(state: State<AppState>) -> CmdResult<()> {
    casebook::clear_fixes(&state.db()).map_err(db_err)
}

#[tauri::command]
pub fn import_fixes(state: State<AppState>, text: String) -> CmdResult<usize> {
    let items = import::parse_fixes(&text)?;
    let mut db = state.db();
    let tx = db.transaction().map_err(db_err)?;
    for item in &items {
        casebook::save_fix(&tx, item).map_err(db_err)?;
    }
    tx.commit().map_err(db_err)?;
    Ok(items.len())
}

// ----- Issue history --------------------------------------------------------

#[tauri::command]
pub fn list_issues(state: State<AppState>) -> CmdResult<Vec<IssueRecord>> {
    casebook::list_issues(&state.db(), 300).map_err(db_err)
}

#[tauri::command]
pub fn delete_issue(state: State<AppState>, id: i64) -> CmdResult<()> {
    casebook::delete_issue(&state.db(), id).map_err(db_err)
}

#[tauri::command]
pub fn clear_issues(state: State<AppState>) -> CmdResult<()> {
    casebook::clear_issues(&state.db()).map_err(db_err)
}

// ----- Clipboard history and cases -------------------------------------------

#[tauri::command]
pub fn list_clipboard(state: State<AppState>) -> CmdResult<Vec<ClipItem>> {
    cliphistory::list(&state.db(), 500).map_err(db_err)
}

#[tauri::command]
pub fn delete_clipboard_item(state: State<AppState>, id: i64) -> CmdResult<()> {
    cliphistory::delete(&state.db(), id).map_err(db_err)
}

#[tauri::command]
pub fn clear_clipboard(state: State<AppState>) -> CmdResult<()> {
    cliphistory::clear(&state.db()).map_err(db_err)
}

#[tauri::command]
pub fn copy_clipboard_item(state: State<AppState>, id: i64) -> CmdResult<()> {
    let item = cliphistory::get_many(&state.db(), &[id]).into_iter().next().ok_or("That item no longer exists.")?;
    crate::platform::pasteboard::write_string(&item.content);
    Ok(())
}

/// Adds picked history items to the multi-message case (oldest first).
#[tauri::command]
pub fn case_add_items(app: AppHandle, state: State<AppState>, ids: Vec<i64>) -> CmdResult<usize> {
    let items = cliphistory::get_many(&state.db(), &ids);
    case::add_texts(&app, items.into_iter().map(|i| i.content).collect());
    Ok(case::len())
}

#[tauri::command]
pub fn case_status() -> usize {
    case::len()
}

#[tauri::command]
pub fn case_clear(app: AppHandle) {
    case::clear(&app);
}

// ----- Analytics and feedback learning ----------------------------------------

#[tauri::command]
pub fn get_dashboard(state: State<AppState>, range_days: i64) -> analytics::Dashboard {
    let s = state.settings();
    analytics::dashboard(&state.db(), range_days, s.minutes_per_reply, s.minutes_per_debug)
}

#[tauri::command]
pub fn reset_analytics(state: State<AppState>) -> CmdResult<()> {
    analytics::reset(&state.db()).map_err(db_err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Learning {
    samples: i64,
    profile: Option<String>,
}

#[tauri::command]
pub fn get_learning(state: State<AppState>) -> Learning {
    Learning { samples: feedback::count(&state.db()), profile: state.edit_profile() }
}

#[tauri::command]
pub fn reset_learning(state: State<AppState>) -> CmdResult<()> {
    feedback::clear(&state.db()).map_err(db_err)?;
    state.refresh_edit_profile();
    Ok(())
}
