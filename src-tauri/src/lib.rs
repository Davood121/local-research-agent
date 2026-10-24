//! Application entry point.
//!
//! The crate-level lint denies enforce the "no panics in command paths" rule for
//! every production module, including the event bus and telemetry. Test modules
//! opt out explicitly because assertions and fixtures are allowed to fail
//! loudly.
//!
//! The Tauri runtime (and therefore the GUI/webview stack) is compiled out of
//! the unit-test harness. That keeps `cargo test` free of native window-system
//! requirements, while `cargo clippy --all-targets` and every real build still
//! type-check the desktop modules. The test harness is also exempt from
//! dead-code analysis, because it deliberately omits the desktop shell.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::todo, clippy::unimplemented)]
// Test builds intentionally exclude the desktop shell (see below), so items
// that only the shell reaches are dead in that configuration.
#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
mod commands;
mod config;
mod engine;
mod error;
mod events;
mod model_catalog;
mod ollama;
mod research;
#[cfg(not(test))]
mod state;
mod telemetry;

#[cfg(not(test))]
use tauri::Manager;

#[cfg(not(test))]
use state::AppState;

#[cfg(not(test))]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(AppState::new())
        .setup(|app| {
            let state = app.state::<AppState>();
            commands::engine::restore_device(app.handle());
            telemetry::spawn(app.handle().clone(), state.tx.clone(), state.cancel.clone());
            Ok(())
        });

    // The debug command is both compiled out and not registered in release, so
    // a shipped webview has no path to it.
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::subscribe::subscribe_events,
        commands::chat::ollama_status,
        commands::chat::start_chat,
        commands::chat::cancel_chat,
        commands::engine::engine_status,
        commands::engine::engine_unload,
        commands::engine::set_inference_device,
        commands::models::list_model_catalog,
        commands::models::download_model,
        commands::models::cancel_model_download,
        commands::research::research_status,
        commands::research::start_research,
        commands::debug_stream::debug_start_stream
    ]);

    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::subscribe::subscribe_events,
        commands::chat::ollama_status,
        commands::chat::start_chat,
        commands::chat::cancel_chat,
        commands::engine::engine_status,
        commands::engine::engine_unload,
        commands::engine::set_inference_device,
        commands::models::list_model_catalog,
        commands::models::download_model,
        commands::models::cancel_model_download,
        commands::research::research_status,
        commands::research::start_research
    ]);

    let app = match builder.build(tauri::generate_context!()) {
        Ok(app) => app,
        Err(error) => {
            // No window exists yet; stderr is the only local sink. Nothing is uploaded.
            eprintln!("local-research-agent: failed to start: {error}");
            return;
        }
    };

    app.run(|handle, event| {
        if matches!(
            event,
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
        ) {
            handle.state::<AppState>().cancel.cancel();
        }
    });
}
