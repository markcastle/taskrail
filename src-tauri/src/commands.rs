//! What the UI can ask the shell to do. Each command is a thin translation
//! from the webview's JSON world into a call on the core or the platform.

use taskrail_core::domain::ItemId;
use taskrail_core::settings::{Edge, RailSettings};
use taskrail_core::usecases::{build_rail_view, RailView};
use tauri::{AppHandle, State};

use crate::state::AppState;

#[tauri::command]
pub fn rail_view(state: State<'_, AppState>) -> RailView {
    let settings = lock(&state.settings).clone();
    let items = lock(&state.items).clone();
    let active = lock(&state.active).clone();
    build_rail_view(&items, active.as_ref(), chrono::Utc::now(), &settings)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> RailSettings {
    lock(&state.settings).clone()
}

#[tauri::command]
pub fn set_edge(app: AppHandle, state: State<'_, AppState>, edge: Edge) -> Result<(), String> {
    lock(&state.settings).edge = edge;
    crate::dock::redock(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_active(state: State<'_, AppState>, id: Option<ItemId>) {
    *lock(&state.active) = id;
}

/// A poisoned mutex here means a command panicked mid-update. The data is
/// plain values, so carrying on is safer than taking the whole rail down.
pub(crate) fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
