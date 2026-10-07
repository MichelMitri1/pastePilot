use crate::platform::hud;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

const BACKGROUND_INTERVAL: Duration = Duration::from_secs(12 * 3600);

static PENDING: Mutex<Option<Update>> = Mutex::new(None);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current: String,
    available: bool,
    version: Option<String>,
    notes: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    let update = app
        .updater()
        .map_err(|e| format!("Updater unavailable: {e}"))?
        .check()
        .await
        .map_err(|_| "Couldn't check for updates. Check your connection and try again.".to_string())?;
    let info = UpdateInfo {
        current,
        available: update.is_some(),
        version: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()).filter(|n| !n.trim().is_empty()),
    };
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = update;
    Ok(info)
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateInfo, String> {
    check(&app).await
}

/// Downloads, verifies and installs the update found by the last check, then restarts.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let update = match pending {
        Some(u) => u,
        None => {
            check(&app).await?;
            PENDING.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or("You're already on the latest version.")?
        }
    };
    let emitter = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = emitter.emit("update-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("The update couldn't be installed: {e}"))?;
    app.restart();
}

/// Quietly checks a little after launch, then twice a day. Only speaks up when there's an update.
pub fn start_background_checks(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if let Ok(info) = check(&app).await {
                if info.available {
                    let version = info.version.unwrap_or_default();
                    hud::show(
                        &app,
                        &format!("PastePilot {version} is available. Menu bar → Check for Updates…"),
                        Some(Duration::from_secs(6)),
                    );
                }
            }
            tokio::time::sleep(BACKGROUND_INTERVAL).await;
        }
    });
}
