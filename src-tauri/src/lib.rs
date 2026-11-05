//! The Task Rail shell: a Tauri app that owns one window, docks it to the
//! screen edge, and answers the UI's questions through commands.
//!
//! The shell is thin on purpose. It opens the store, builds the service from
//! the core, hands the window to the platform crate for docking, and
//! translates between the webview's JSON and the core's types. Product rules
//! live in the core; OS rules live in the platform crate.

mod commands;
mod connectors;
mod dock;
mod model;
mod rail;
mod secrets;
mod seed;
mod settings_window;
mod shortcuts;
mod state;
mod sync;
mod theme;
mod tray;

pub use state::AppState;

use tauri::{Manager, RunEvent};

pub fn run() {
    let app = tauri::Builder::default()
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
            commands::quit,
            commands::open_settings,
            commands::list_connectors,
            commands::add_connector_profile,
            commands::add_demo_connector,
            commands::remove_connector,
            commands::sync_connector,
            commands::get_model_settings,
            commands::save_model_settings,
            commands::test_model,
            commands::discover_connector,
            commands::sign_in,
        ])
        .setup(|app| {
            let rail = rail::Rail::open(data_file(app.handle()))?;
            let connectors = connectors::ConnectorRegistry::load(rail.service.repository());
            app.manage(AppState::new(rail, connectors));
            dock::attach(app.handle())?;
            theme::apply(app.handle());
            shortcuts::register(app.handle());
            tray::install(app.handle())?;
            release_on_console_signal(app.handle().clone());
            sync::start(app.handle().clone());
            // Debugging aid: `TASKRAIL_OPEN_SETTINGS=1` opens the settings
            // window straight away, for looking at it under a test display.
            if std::env::var_os("TASKRAIL_OPEN_SETTINGS").is_some() {
                settings_window::open(app.handle())?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Task Rail failed to start");

    app.run(|app, event| {
        // Give the reserved strip back before the process goes away. Windows
        // would notice eventually; this makes it immediate.
        if let RunEvent::Exit = event {
            dock::release(app);
        }
    });
}

/// Ctrl+C in the console that launched a dev build, or that console closing,
/// would otherwise kill the process before it gives the reserved strip back,
/// and Windows then keeps the desktop shrunk until something re-registers.
fn release_on_console_signal(app: tauri::AppHandle) {
    if let Err(e) = ctrlc::set_handler(move || {
        dock::release(&app);
        std::process::exit(0);
    }) {
        eprintln!("task rail: could not install the console signal handler: {e}");
    }
}

/// `rail.sqlite` in the per-user app data directory, or `None` to run on an
/// in-memory database when that directory cannot be created.
fn data_file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("rail.sqlite"))
}
