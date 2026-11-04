//! The Task Rail shell: a Tauri app that owns one window, docks it to the
//! screen edge, and answers the UI's questions through commands.
//!
//! The shell is thin on purpose. It knows how to get a native window handle,
//! how to call into `taskrail-core` for the view, and how to hand the window
//! to `taskrail-platform` for docking. Product rules live in the core; OS
//! rules live in the platform crate.

mod commands;
mod dock;
mod fixtures;
mod state;

pub use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::rail_view,
            commands::get_settings,
            commands::set_edge,
            commands::set_active,
        ])
        .setup(|app| {
            dock::attach(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Task Rail failed to start");
}
