//! Window connection state and preferences. Unavailable transport is a separate
//! state, never an empty list of agents or an instruction to recover their turns.

use std::sync::Arc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::desktop::Desktop;
use crate::{AppState, store::settings::{self, QuitBehavior}};

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ConnectionState {
    InProcess,
    Connected { epoch: String, running_agents: usize, terminals: usize },
    Unavailable { message: String },
}

fn connected(runtime: &Desktop) -> anyhow::Result<ConnectionState> {
    let running_agents = runtime.sessions()?.running_tabs()?.len();
    let terminals = runtime.terminals()?.shells()?.iter().filter(|shell| !shell.exited).count();
    let epoch = runtime.attachment()?.epoch;
    Ok(ConnectionState::Connected { epoch, running_agents, terminals })
}

#[tauri::command]
pub async fn local_runtime_status(state: State<'_, AppState>) -> Result<ConnectionState, String> {
    let Some(runtime) = state.runtime() else { return Ok(ConnectionState::InProcess) };
    tauri::async_runtime::spawn_blocking(move || connected(&runtime).unwrap_or_else(|error| ConnectionState::Unavailable { message: format!("{error:#}") }))
        .await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn local_runtime_reconnect(app: AppHandle) -> Result<ConnectionState, String> {
    let previous = app.state::<AppState>().runtime().ok_or("The background runtime is not attached.")?;
    static RECONNECT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _reconnect = RECONNECT.lock().await;
    if !app.state::<AppState>().runtime().as_ref().is_some_and(|current| Arc::ptr_eq(current, &previous)) {
        return Err("The desktop connection changed while reconnecting. Try again.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let next = Arc::new(previous.reconnect().map_err(|error| format!("Could not reconnect to the running owner: {error:#}. Reopen TerminalX if it has stopped."))?);
        let status = connected(&next).map_err(|error| format!("{error:#}"))?;
        {
            let state = app.state::<AppState>();
            let mut current = state.runtime.lock().unwrap();
            if !current.as_ref().is_some_and(|current| Arc::ptr_eq(current, &previous)) {
                return Err("The desktop connection changed while reconnecting. Try again.".into());
            }
            state.pairing.attach_runtime(next.clone());
            *current = Some(next);
        }
        let _ = app.emit("local_runtime_reattached", ());
        Ok(status)
    }).await.map_err(|error| error.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSettings { quit_behavior: Option<QuitBehavior> }

#[tauri::command]
pub fn local_runtime_settings() -> RuntimeSettings {
    RuntimeSettings { quit_behavior: settings::load().quit_behavior }
}

#[tauri::command]
pub fn set_local_runtime_quit_behavior(quit_behavior: QuitBehavior) -> Result<RuntimeSettings, String> {
    let mut settings = settings::load();
    settings.quit_behavior = Some(quit_behavior);
    settings::save(&settings).map_err(|error| format!("{error:#}"))?;
    Ok(RuntimeSettings { quit_behavior: settings.quit_behavior })
}
