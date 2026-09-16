// 托盘常驻（spec 架构承诺）：关窗不退出而是 hide，轮询继续
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开仪表盘", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        });
    // 脚手架自带图标先用，mac 构建时再换 template 版；无图标不 panic，托盘照常可建
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
}

/// 关窗不退出（hide），托盘常驻继续轮询——这是 spec 架构承诺的验收项。
/// Tauri 2 的 Builder::on_window_event 回调签名是 (&Window, &WindowEvent)，
/// 与 brief 的 (&WebviewWindow, ...) 不同，此处按实际 API 修正。
pub fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() != "main" { return; }
        api.prevent_close();
        let _ = window.hide();
    }
}
