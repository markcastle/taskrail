//! The Task Rail shell: a Tauri app that owns one window, docks it to the
//! screen edge, and answers the UI's questions through commands.
//!
//! The shell is thin on purpose. It opens the store, builds the service from
//! the core, hands the window to the platform crate for docking, and
//! translates between the webview's JSON and the core's types. Product rules
//! live in the core; OS rules live in the platform crate.

mod commands;
mod dock;
mod rail;
mod seed;
mod shortcuts;
mod state;

pub use state::AppState;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(shortcuts::plugin())
        .invoke_handler(tauri::generate_handler![
            commands::rail_view,
            commands::add_task,
            commands::start_task,
            commands::complete_task,
            commands::reopen_task,
            commands::move_task,
            commands::remove_task,
            commands::set_active,
            commands::get_settings,
            commands::update_settings,
        ])
        .setup(|app| {
            let rail = rail::Rail::open(data_file(app.handle()))?;
            app.manage(AppState::new(rail));
            dock::attach(app.handle())?;
            shortcuts::register(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Task Rail failed to start");
}

/// `rail.sqlite` in the per-user app data directory, or `None` to run on an
/// in-memory database when that directory cannot be created.
fn data_file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("rail.sqlite"))
}
