pub mod commands;
pub mod db;
pub mod error;
pub mod events;
pub mod limits;
pub mod providers;
pub mod queue;
pub mod secrets;
pub mod services;
pub mod state;
pub mod util;

use events::EventSink;
use state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager, WindowEvent};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

/// Forwards backend events to the React UI.
struct TauriEvents(tauri::AppHandle);

impl EventSink for TauriEvents {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        let _ = self.0.emit(name, payload);
    }
}

/// Appends a line to `startup.log` in the app data folder. Used for panics and startup failures,
/// because the release build has no console window to show them.
pub(crate) fn crash_log(msg: &str) {
    use std::io::Write;
    let dir = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("com.myarticlemanager.app");
    let _ = std::fs::create_dir_all(&dir);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("startup.log")) {
        let _ = writeln!(f, "[unix {secs}] {msg}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Panics (including the setup error reported by `expect` below) go to startup.log.
    std::panic::set_hook(Box::new(|info| crash_log(&format!("panic: {info}"))));
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Closing the main window hides it; the scheduler keeps running.
            let open = MenuItem::with_id(app, "open", "باز کردن برنامه", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "خروج کامل", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("application icon").clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            let dir = app.path().app_data_dir()?;
            let events: Arc<dyn EventSink> = Arc::new(TauriEvents(app.handle().clone()));
            let state = tauri::async_runtime::block_on(AppState::init(dir, secrets::default_store(), events))?;
            let online = state.online.clone();

            // Never auto-resume generation after reopening the application.
            tauri::async_runtime::block_on(async {
                sqlx::query("UPDATE campaigns SET status='PAUSED' WHERE status='IN_PROGRESS'")
                    .execute(&state.db).await
            })?;
            // Crash recovery, then start the background job scheduler.
            let recovered = tauri::async_runtime::block_on(queue::recover(&state))?;
            if recovered > 0 {
                tracing::info!("recovered {recovered} interrupted jobs");
            }
            let sched_state = state.clone();
            tauri::async_runtime::spawn(queue::Scheduler::new().run(sched_state));
            app.manage(state);

            // Connectivity monitor: emits `connectivity:changed` whenever the state flips.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last: Option<bool> = None;
                loop {
                    let now = services::system::probe_internet().await;
                    online.store(now, Ordering::Relaxed);
                    if last != Some(now) {
                        last = Some(now);
                        let _ = handle.emit("connectivity:changed", serde_json::json!({ "online": now }));
                    }
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_app_info,
            commands::license::get_license_status,
            commands::license::activate_license,
            commands::system::reset_application,
            commands::system::check_internet,
            commands::system::get_dashboard_stats,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::create_project,
            commands::projects::update_project,
            commands::projects::delete_project,
            commands::wordpress::get_wordpress_connection,
            commands::wordpress::save_wordpress_connection,
            commands::wordpress::test_wordpress_connection,
            commands::wordpress::sync_wordpress_taxonomies,
            commands::wordpress::list_wp_terms,
            commands::network::get_network_settings,
            commands::network::save_network_settings,
            commands::products::list_products,
            commands::products::generate_product_description,
            commands::products::save_product_description,
            commands::ai_providers::list_ai_providers,
            commands::ai_providers::save_ai_provider,
            commands::ai_providers::delete_ai_provider,
            commands::ai_providers::test_ai_provider,
            commands::campaigns::list_campaigns,
            commands::campaigns::get_campaign,
            commands::campaigns::create_campaign,
            commands::campaigns::delete_campaign,
            commands::campaigns::save_wizard_state,
            commands::campaigns::set_campaign_ai_provider,
            commands::campaigns::generate_secondary_keywords,
            commands::campaigns::confirm_secondary_keywords,
            commands::campaigns::reset_article_plans,
            commands::keywords::save_primary_keywords,
            commands::keywords::import_keywords_csv,
            commands::keywords::add_manual_keyword,
            commands::keywords::add_secondary_keyword,
            commands::keywords::edit_campaign_keyword,
            commands::keywords::ai_suggest_keywords,
            commands::keywords::list_keywords,
            commands::keywords::select_keywords,
            commands::keywords::delete_keywords,
            commands::keywords::get_similarity_warnings,
            commands::titles::generate_titles,
            commands::titles::list_article_plans,
            commands::titles::approve_title,
            commands::titles::edit_title,
            commands::titles::reject_title,
            commands::titles::generate_alternative_title,
            commands::titles::approve_all_titles,
            commands::templates::list_templates,
            commands::templates::get_template,
            commands::templates::save_template,
            commands::templates::duplicate_template,
            commands::templates::delete_template,
            commands::image_providers::list_image_providers,
            commands::image_providers::save_image_provider,
            commands::image_providers::delete_image_provider,
            commands::image_providers::test_image_provider,
            commands::generation::set_campaign_template,
            commands::generation::set_campaign_image_provider,
            commands::generation::start_generation,
            commands::generation::pause_campaign,
            commands::generation::resume_campaign,
            commands::generation::cancel_campaign,
            commands::generation::get_progress,
            commands::generation::list_articles,
            commands::generation::set_upload_schedule,
            commands::generation::get_article,
            commands::generation::get_article_image,
            commands::generation::get_article_preview_images,
            commands::generation::retry_article,
            commands::generation::retry_failed,
            commands::generation::preview_prompt,
            commands::generation::get_queue_settings,
            commands::generation::update_queue_settings,
            commands::tools::upload_to_wordpress,
            commands::tools::publish_uploaded_now,
            commands::tools::clear_cache,
            commands::quota::get_quota,
            commands::single::create_single_article,
            commands::single::suggest_single_titles,
            commands::single::list_single_articles,
            commands::prefs::get_app_prefs,
            commands::prefs::save_app_prefs,
            commands::tools::upload_campaign,
            commands::tools::set_plan_taxonomy,
            commands::tools::set_default_category,
            commands::tools::update_article,
            commands::tools::preview_html,
            commands::tools::list_logs,
            commands::tools::list_log_actions,
            commands::tools::export_logs,
            commands::tools::export_backup,
            commands::tools::preview_backup,
            commands::tools::import_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SEO Master");
}
