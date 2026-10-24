//! The macOS menu bar: the standard menus, with Settings in the app menu (Cmd+,).

use tauri::menu::{Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::{AppHandle, Manager, Runtime};

const SETTINGS: &str = "settings";
/// The page opens Settings on this event; see `App.tsx`.
const OPEN_SETTINGS: &str = "window.dispatchEvent(new CustomEvent('opencrate:open-settings'))";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    let settings = MenuItem::with_id(app, SETTINGS, "Settings...", true, Some("CmdOrCtrl+,"))?;
    // The first menu is the app's own, after About.
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        app_menu.insert_items(&[&PredefinedMenuItem::separator(app)?, &settings], 1)?;
    }
    Ok(menu)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a menu handler its event by value"
)]
pub fn on_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    if event.id() == SETTINGS {
        for window in app.webview_windows().values() {
            let _ = window.eval(OPEN_SETTINGS);
        }
    }
}
