//! The Task Rail shell: a Tauri app that owns one window, docks it to the
//! screen edge, and answers the UI's questions through commands.
//!
//! The shell is thin on purpose. It opens the store, builds the service from
//! the core, hands the window to the platform crate for docking, and
//! translates between the webview's JSON and the core's types. Product rules
//! live in the core; OS rules live in the platform crate.

mod autohide;
mod blueprints;
mod commands;
mod connectors;
mod dock;
mod logs;
mod model;
mod rail;
mod secrets;
mod seed;
mod shortcuts;
mod startup;
mod state;
mod sync;
mod theme;
mod tray;
mod update;

pub use state::AppState;

use tauri::{Manager, RunEvent};

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(logs::plugin())
        .plugin(shortcuts::plugin())
        .plugin(startup::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            commands::expand_rail,
            commands::reveal_rail,
            commands::rail_phase,
            commands::hold_rail,
            commands::preview_width,
            commands::list_monitors,
            commands::list_connectors,
            commands::add_connector_profile,
            commands::add_demo_connector,
            commands::add_calendar,
            commands::remove_connector,
            commands::set_connector_paused,
            commands::connector_sample,
            commands::get_connector_scope,
            commands::set_connector_scope,
            commands::sync_connector,
            commands::get_model_settings,
            commands::save_model_settings,
            commands::test_model,
            commands::discover_connector,
            commands::sign_in,
            commands::read_log,
            commands::app_version,
            commands::log_from_ui,
            commands::open_log_folder,
            commands::export_blueprint,
            commands::open_project_page,
            commands::check_for_update,
            commands::install_update,
        ])
        .setup(|app| {
            log::info!(
                "Task Rail {} ({}) starting",
                env!("CARGO_PKG_VERSION"),
                env!("TASKRAIL_COMMIT")
            );
            if let Some(path) = logs::file(app.handle()) {
                log::info!("log file: {}", path.display());
            }
            let rail = rail::Rail::open(data_file(app.handle()))?;
            let connectors = connectors::ConnectorRegistry::load(rail.service.repository());
            app.manage(AppState::new(rail, connectors));
            // A rail that will not start at login is still a rail.
            if let Err(e) = startup::apply(app.handle()) {
                log::warn!("{e}");
            }
            dock::attach(app.handle())?;
            theme::apply(app.handle());
            shortcuts::register(app.handle());
            tray::install(app.handle())?;
            release_on_console_signal(app.handle().clone());
            sync::start(app.handle().clone());
            update::start(app.handle().clone());
            autohide::start(app.handle().clone());
            blueprints::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            use tauri::WindowEvent::{Moved, Resized, ScaleFactorChanged};
            if window.label() == dock::RAIL_WINDOW
                && matches!(event, Moved(_) | Resized(_) | ScaleFactorChanged { .. })
            {
                dock::note_window_rect(window.app_handle());
            }
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
        log::warn!("could not install the console signal handler: {e}");
    }
}

/// `rail.sqlite` in the per-user app data directory, or `None` to run on an
/// in-memory database when that directory cannot be created.
fn data_file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("rail.sqlite"))
}
