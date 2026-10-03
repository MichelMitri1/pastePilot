//! Shared app state: settings, the cached system prompt, the cached API key,
//! the database, and the last generated reply (for rewrites).

use crate::db::{self, Db};
use crate::memory::Turn;
use crate::modes::Mode;
use crate::openai::OpenAi;
use crate::settings::{self, Settings};
use crate::{keychain, prompt, retrieval};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Manager, Wry};

/// Everything needed to rewrite the last reply without rebuilding context.
#[derive(Clone)]
pub struct LastReply {
    pub target_pid: i32,
    /// Conversation scope the reply belongs to (to tell a send from a ticket switch).
    pub scope_key: Option<String>,
    pub student_message_id: Option<i64>,
    pub agent_message_id: Option<i64>,
    pub student_text: String,
    /// The formatted user message that was sent.
    pub user: String,
    pub history: Vec<Turn>,
    /// The per-request context (mode + knowledge + examples) that was sent.
    pub context: String,
    pub mode: Mode,
    pub reply: String,
}

#[derive(Default)]
pub struct Menus {
    pub auto_paste: Option<CheckMenuItem<Wry>>,
    pub memory: Option<CheckMenuItem<Wry>>,
    /// Index 0 = Auto, then Mode::ALL order.
    pub modes: Vec<CheckMenuItem<Wry>>,
}

pub struct AppState {
    settings_path: PathBuf,
    settings: RwLock<Settings>,
    style_profile: RwLock<Option<String>>,
    system_prompt: RwLock<Arc<str>>,
    /// Loaded from Keychain once, then served from memory (no Keychain hit per reply).
    api_key: Mutex<Option<String>>,
    db: Mutex<Db>,
    pub db_error: Option<String>,
    pub openai: OpenAi,
    pub menus: Mutex<Menus>,
    pub current_conversation: Mutex<Option<i64>>,
    pub last_reply: Mutex<Option<LastReply>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn load(app: &AppHandle) -> Self {
        let dir = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir());
        let settings_path = dir.join("settings.json");
        let settings = settings::load(&settings_path);

        let (db, db_error) = match db::open(&dir.join("pastepilot.db")) {
            Ok(db) => (db, None),
            Err(e) => (db::open_in_memory().expect("in-memory sqlite"), Some(format!("Database unavailable: {e}"))),
        };
        let style_profile = retrieval::style_profile(&db::all_example_replies(&db).unwrap_or_default());
        let system_prompt: Arc<str> = prompt::build_system_prompt(&settings, style_profile.as_deref()).into();

        // Keychain first; OPENAI_API_KEY is a development fallback only.
        let api_key = keychain::load().or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty()));
        Self {
            settings_path,
            settings: RwLock::new(settings),
            style_profile: RwLock::new(style_profile),
            system_prompt: RwLock::new(system_prompt),
            api_key: Mutex::new(api_key),
            db: Mutex::new(db),
            db_error,
            openai: OpenAi::new(),
            menus: Mutex::new(Menus::default()),
            current_conversation: Mutex::new(None),
            last_reply: Mutex::new(None),
        }
    }

    pub fn db(&self) -> MutexGuard<'_, Db> {
        lock(&self.db)
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn system_prompt(&self) -> Arc<str> {
        self.system_prompt.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn rebuild_system_prompt(&self) {
        let settings = self.settings();
        let profile = self.style_profile.read().unwrap_or_else(|e| e.into_inner()).clone();
        *self.system_prompt.write().unwrap_or_else(|e| e.into_inner()) =
            prompt::build_system_prompt(&settings, profile.as_deref()).into();
    }

    pub fn replace_settings(&self, new: Settings) -> Result<(), String> {
        settings::save(&self.settings_path, &new).map_err(|e| format!("Couldn't save settings: {e}"))?;
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = new;
        self.rebuild_system_prompt();
        Ok(())
    }

    /// Call after reply examples change: re-measures the style profile.
    pub fn refresh_style_profile(&self) {
        let replies = db::all_example_replies(&self.db()).unwrap_or_default();
        *self.style_profile.write().unwrap_or_else(|e| e.into_inner()) = retrieval::style_profile(&replies);
        self.rebuild_system_prompt();
    }

    pub fn api_key(&self) -> Option<String> {
        lock(&self.api_key).clone()
    }

    pub fn set_api_key(&self, key: &str) -> Result<(), String> {
        keychain::store(key)?;
        *lock(&self.api_key) = Some(key.to_owned());
        Ok(())
    }

    pub fn clear_api_key(&self) {
        keychain::delete();
        *lock(&self.api_key) = None;
    }

    pub fn last_reply(&self) -> Option<LastReply> {
        lock(&self.last_reply).clone()
    }

    pub fn set_last_reply(&self, last: Option<LastReply>) {
        *lock(&self.last_reply) = last;
    }

    pub fn current_conversation(&self) -> Option<i64> {
        *lock(&self.current_conversation)
    }

    pub fn set_current_conversation(&self, id: Option<i64>) {
        *lock(&self.current_conversation) = id;
    }
}
