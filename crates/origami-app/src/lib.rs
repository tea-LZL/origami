mod commands;
pub mod display_lru;
mod notifications;
mod oauth_flow;
mod state;

use origami_core::model::Flag;
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

/// Minimum gap between tray badge refreshes during a new-mail burst. The
/// unread count is a full-table scan; bursts must not run one per message.
const BADGE_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

fn badge_update_due(last: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= BADGE_MIN_INTERVAL)
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

            let tray_slot: std::sync::Arc<std::sync::Mutex<Option<tauri::tray::TrayIcon>>> =
                std::sync::Arc::new(std::sync::Mutex::new(None));

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
                let tray = TrayIconBuilder::with_id("origami-tray")
                    .icon(icon)
                    .menu(&menu)
                    .tooltip("Origami")
                    .on_menu_event(move |app, event| {
                        if event.id == open_id {
                            show_main_window(app);
                        } else if event.id == quit_id {
                            let state = app.state::<state::AppState>();
                            state.stop_all_sync();
                            if let Err(error) = state.store.shutdown_flush() {
                                tracing::warn!("wal checkpoint on quit failed: {error}");
                            }
                            app.exit(0);
                        }
                    })
                    .build(app)?;
                tray_slot.lock().unwrap().replace(tray);
            }

            let state = app.state::<state::AppState>();
            let mut events = state.engine.subscribe();
            let handle = app.handle().clone();
            let account_errors = state.account_errors.clone();
            let syncing_accounts = state.syncing_accounts.clone();
            let badge_tray = tray_slot.clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                let mut bookkeeping = notifications::NotificationBookkeeping::new(1);
                let mut last_badge_at: Option<std::time::Instant> = None;

                fn update_unread_badge(
                    handle: &tauri::AppHandle,
                    tray: &std::sync::Mutex<Option<tauri::tray::TrayIcon>>,
                ) {
                    let state = handle.state::<state::AppState>();
                    let unread = state.store.total_unread().unwrap_or(0);
                    let tooltip = if unread > 0 {
                        format!("Origami — {unread} unread")
                    } else {
                        "Origami".to_string()
                    };
                    if let Some(tray) = tray.lock().unwrap().as_ref() {
                        let _ = tray.set_tooltip(Some(tooltip));
                    }
                }

                fn account_name(handle: &tauri::AppHandle, account_id: &str) -> String {
                    handle
                        .state::<state::AppState>()
                        .read_config()
                        .accounts
                        .get(account_id)
                        .map(|account| account.name.clone())
                        .unwrap_or_else(|| account_id.to_string())
                }

                // Trailing badge refresh: a burst that lands inside the debounce
                // window still ends with an accurate count.
                let badge_dirty = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                {
                    let handle = handle.clone();
                    let tray = badge_tray.clone();
                    let dirty = badge_dirty.clone();
                    tauri::async_runtime::spawn(async move {
                        loop {
                            tokio::time::sleep(BADGE_MIN_INTERVAL).await;
                            if dirty.swap(false, std::sync::atomic::Ordering::Relaxed) {
                                update_unread_badge(&handle, &tray);
                            }
                        }
                    });
                }
                loop {
                    match events.recv().await {
                        Ok(event) => {
                            let received = std::time::Instant::now();
                            tracing::debug!(
                                target: "origami::timing",
                                phase = "event_received",
                                "timing"
                            );
                            // Status bookkeeping lands before the UI is told:
                            // the event makes the UI poll these maps.
                            match &event {
                                origami_core::sync::SyncEvent::AccountSyncStarted {
                                    account_id,
                                } => {
                                    syncing_accounts.lock().unwrap().insert(account_id.clone());
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
                                    bookkeeping.forget_account(account_id);
                                }
                                // An INBOX-only wake emits FolderSynced but no
                                // AccountSynced; a successful folder sync is
                                // proof the account is reachable again.
                                origami_core::sync::SyncEvent::FolderSynced {
                                    account_id, ..
                                } => {
                                    account_errors.lock().unwrap().remove(account_id);
                                }
                                _ => {}
                            }
                            if let Err(e) = handle.emit("sync-event", &event) {
                                tracing::warn!("cannot emit sync event: {e}");
                            }
                            tracing::debug!(
                                target: "origami::timing",
                                phase = "event_forwarded",
                                forward_ms = received.elapsed().as_millis() as u64,
                                "timing"
                            );
                            // Notifications and the badge are advisory. Never
                            // let DBus or the unread count delay the list.
                            if let origami_core::sync::SyncEvent::NewEnvelope {
                                account_id,
                                folder,
                                envelope,
                            } = &event
                            {
                                let state = handle.state::<state::AppState>();
                                let settings = state.read_config().notifications;
                                let folder_role = state.store.folder_role(folder).ok().flatten();
                                let logical_id = envelope.logical_id(account_id);
                                let unread = !envelope.flags.contains(&Flag::Seen);
                                let from = envelope
                                    .from
                                    .first()
                                    .map(|a| a.name.clone().unwrap_or_else(|| a.addr.clone()))
                                    .unwrap_or_default();
                                let mut show = |draft: notifications::NotificationDraft| {
                                    // notify_rust blocks on DBus; keep it off
                                    // the event loop.
                                    std::mem::drop(tauri::async_runtime::spawn_blocking(
                                        move || {
                                            notifications::show_notification(&draft);
                                        },
                                    ));
                                };
                                let notified = notifications::notify_new_mail(
                                    &mut bookkeeping,
                                    &settings,
                                    &account_name(&handle, account_id),
                                    account_id,
                                    &logical_id,
                                    folder_role,
                                    unread,
                                    &envelope.subject,
                                    &from,
                                    &mut show,
                                );
                                let now = std::time::Instant::now();
                                if notified {
                                    if badge_update_due(last_badge_at, now) {
                                        update_unread_badge(&handle, &badge_tray);
                                        last_badge_at = Some(now);
                                        badge_dirty
                                            .store(false, std::sync::atomic::Ordering::Relaxed);
                                    } else {
                                        // Suppressed by the debounce: let the
                                        // ticker correct the count.
                                        badge_dirty
                                            .store(true, std::sync::atomic::Ordering::Relaxed);
                                    }
                                }
                            }
                            if let origami_core::sync::SyncEvent::AccountSynced { .. } = &event {
                                update_unread_badge(&handle, &badge_tray);
                                last_badge_at = Some(std::time::Instant::now());
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
            commands::set_folder_subscribed,
            commands::rename_folder,
            commands::delete_folder,
            commands::list_envelopes,
            commands::list_unified_inbox,
            commands::get_cached_message,
            commands::get_message,
            commands::get_attachment,
            commands::open_attachment,
            commands::store_flags,
            commands::store_flags_batch,
            commands::store_keywords_batch,
            commands::move_messages,
            commands::delete_messages,
            commands::sync_now,
            commands::prefetch_display,
            commands::prefetch_selected_folder,
            commands::search,
            commands::search_count,
            commands::thread_envelopes,
            commands::search_page,
            commands::list_saved_searches,
            commands::save_search,
            commands::delete_saved_search,
            commands::list_keywords,
            commands::list_correspondents,
            commands::save_composer_draft,
            commands::list_composer_drafts,
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
            commands::reopen_outbox_entry,
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
    use super::{badge_update_due, is_navigation_allowed, BADGE_MIN_INTERVAL};
    use std::time::{Duration, Instant};

    #[test]
    fn first_badge_update_is_due() {
        assert!(badge_update_due(None, Instant::now()));
    }

    #[test]
    fn badge_update_is_debounced_between_burst_messages() {
        let now = Instant::now();
        assert!(!badge_update_due(Some(now), now));
        assert!(!badge_update_due(
            Some(now),
            now + BADGE_MIN_INTERVAL - Duration::from_millis(1)
        ));
        assert!(badge_update_due(Some(now), now + BADGE_MIN_INTERVAL));
    }

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
