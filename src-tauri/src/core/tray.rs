use tauri::{
    AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

/// Set up the system tray icon with context menu.
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let connect_item = MenuItem::with_id(app, "connect", "Connect", true, None::<&str>)?;
    let disconnect_item = MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let show_item = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit NeoCensor", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&connect_item, &disconnect_item, &separator, &show_item, &quit_item],
    )?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("NeoCensor — Disconnected")
        .on_menu_event(move |app_handle, event| {
            let id = event.id.as_ref();
            tracing::debug!("tray menu event: {id}");
            match id {
                "connect" => {
                    let _ = app_handle.emit("tray-action", "connect");
                }
                "disconnect" => {
                    let _ = app_handle.emit("tray-action", "disconnect");
                }
                "show" => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        window.show().ok();
                        window.set_focus().ok();
                    }
                }
                "quit" => {
                    tracing::info!("quit requested from tray");
                    app_handle.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                let app_handle = tray.app_handle();
                if let Some(window) = app_handle.get_webview_window("main") {
                    window.show().ok();
                    window.set_focus().ok();
                }
            }
        })
        .build(app)?;

    tracing::info!("system tray initialized");
    Ok(())
}
