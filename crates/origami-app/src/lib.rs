mod commands;
mod notifications;
mod oauth_flow;
mod state;

use origami_core::model::Flag;
use std::collections::HashSet;
use tauri::Manager;

#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
#[cfg(desktop)]
use tauri::tray::TrayIconBuilder;
#[cfg(desktop)]
use tauri::WindowEvent;

fn is_navigation_allowed(url: &url::Url) -> bool {
    matches!(url.scheme(), "tauri")
        || url.host_str() == Some("tauri.localhost")
        || (cfg!(debug_assertions)
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1")))
        // Sandboxed email `srcdoc` documents navigate as about:srcdoc.
        || url.as_str() == "about:srcdoc"
}

#[cfg(desktop)]
fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("cannot restore Origami: main window is missing");
        return;
    };
    if let Err(error) = window.show() {
        tracing::warn!("cannot show Origami window: {error}");
    }
    if let Err(error) = window.unminimize() {
        tracing::warn!("cannot unminimize Origami window: {error}");
    }
    if let Err(error) = window.set_focus() {
        tracing::warn!("cannot focus Origami window: {error}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "origami=info".into()),
        )
        .init();

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app)
        }));
    }

    builder
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("navigation-guard")
                .on_navigation(|webview, url| {
                    if webview.label() != "main" {
                        return true;
                    }
                    is_navigation_allowed(url)
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_state = state::AppState::new().map_err(|error| {
                std::io::Error::other(format!("cannot initialize Origami state: {error}"))
            })?;
            app.manage(app_state);

            #[cfg(desktop)]
            {
                let window = app
                    .get_webview_window("main")
                    .ok_or_else(|| "main window was not created".to_string())?;
                let hidden_window = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Err(error) = hidden_window.hide() {
                            tracing::warn!("cannot hide Origami window: {error}");
                        }
                    }
                });

                let open =
                    MenuItem::with_id(app, "open-origami", "Open Origami", true, None::<&str>)?;
                let separator = PredefinedMenuItem::separator(app)?;
                let quit =
                    MenuItem::with_id(app, "quit-origami", "Quit Origami", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&open, &separator, &quit])?;
                let open_id = open.id().clone();
                let quit_id = quit.id().clone();
                let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
                TrayIconBuilder::with_id("origami-tray")
                    .icon(icon)
                    .menu(&menu)
                    .tooltip("Origami")
                    .on_menu_event(move |app, event| {
                        if event.id == open_id {
                            show_main_window(app);
                        } else if event.id == quit_id {
                            app.state::<state::AppState>().stop_all_sync();
                            app.exit(0);
                        }
                    })
                    .build(app)?;
            }

            let state = app.state::<state::AppState>();
            let mut events = state.engine.subscribe();
            let handle = app.handle().clone();
            let account_errors = state.account_errors.clone();
            let syncing_accounts = state.syncing_accounts.clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                let mut notified_messages = HashSet::new();
                loop {
                    match events.recv().await {
                        Ok(event) => {
                            match &event {
                                origami_core::sync::SyncEvent::AccountSyncStarted {
                                    account_id,
                                } => {
                                    syncing_accounts.lock().unwrap().insert(account_id.clone());
                                }
                                origami_core::sync::SyncEvent::NewEnvelope {
                                    account_id,
                                    folder,
                                    envelope,
                                } => {
                                    let state = handle.state::<state::AppState>();
                                    let settings = state.read_config().notifications;
                                    let folder_role =
                                        state.store.folder_role(folder).ok().flatten();
                                    let logical_id = envelope.logical_id(account_id);
                                    let unread = !envelope.flags.contains(&Flag::Seen);
                                    if notifications::should_notify(&settings, folder_role, unread)
                                        && notifications::claim_notification(
                                            &mut notified_messages,
                                            &logical_id,
                                        )
                                    {
                                        let from = envelope
                                            .from
                                            .first()
                                            .map(|a| {
                                                a.name.clone().unwrap_or_else(|| a.addr.clone())
                                            })
                                            .unwrap_or_default();
                                        notifications::new_mail_notification(
                                            &settings,
                                            folder_role,
                                            unread,
                                            &envelope.subject,
                                            &from,
                                        );
                                    }
                                }
                                origami_core::sync::SyncEvent::Error {
                                    account_id,
                                    message,
                                    ..
                                } => {
                                    syncing_accounts.lock().unwrap().remove(account_id);
                                    account_errors
                                        .lock()
                                        .unwrap()
                                        .insert(account_id.clone(), message.clone());
                                }
                                origami_core::sync::SyncEvent::AccountSynced {
                                    account_id, ..
                                } => {
                                    syncing_accounts.lock().unwrap().remove(account_id);
                                    account_errors.lock().unwrap().remove(account_id);
                                }
                                _ => {}
                            }
                            if let Err(e) = handle.emit("sync-event", &event) {
                                tracing::warn!("cannot emit sync event: {e}");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("UI lagged {n} sync events behind");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            // Subscribe before starting sync so initial lifecycle events
            // cannot be missed by the status tracker.
            state.spawn_sync_loops();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_accounts,
            commands::list_folders,
            commands::create_folder,
            commands::rename_folder,
            commands::delete_folder,
            commands::list_envelopes,
            commands::list_unified_inbox,
            commands::get_message,
            commands::get_attachment,
            commands::store_flags,
            commands::store_flags_batch,
            commands::store_keywords_batch,
            commands::move_messages,
            commands::delete_messages,
            commands::sync_now,
            commands::search,
            commands::search_page,
            commands::list_saved_searches,
            commands::save_search,
            commands::delete_saved_search,
            commands::list_correspondents,
            commands::save_composer_draft,
            commands::load_composer_draft,
            commands::sync_composer_draft,
            commands::delete_composer_draft,
            commands::send_message,
            commands::provider_hints,
            commands::oauth_client_id,
            commands::oauth_sign_in,
            commands::add_account,
            commands::remove_account,
            commands::account_statuses,
            commands::list_outbox,
            commands::retry_outbox,
            commands::get_notification_settings,
            commands::update_notification_settings,
            commands::get_account_settings,
            commands::update_account,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Origami");
}

#[cfg(test)]
mod tests {
    use super::is_navigation_allowed;

    #[test]
    fn allows_sandboxed_email_srcdoc_navigation() {
        assert!(is_navigation_allowed(
            &url::Url::parse("about:srcdoc").unwrap()
        ));
    }

    #[test]
    fn still_rejects_untrusted_navigation() {
        assert!(!is_navigation_allowed(
            &url::Url::parse("https://example.com").unwrap()
        ));
    }
}
