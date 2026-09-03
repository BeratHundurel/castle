use std::{rc::Rc, time::Duration};

use anyhow::{Context as _, Result};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use gpui::{AnyWindowHandle, App, Global, Window, WindowHandle};
use raw_window_handle::RawWindowHandle;
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem},
};

use quick_capture::{NoteCreatedHandler, QuickCaptureView, WindowVisibilityHandler};
use settings::AppSettings;

struct TrayController {
    window: AnyWindowHandle,
    hotkey_manager: GlobalHotKeyManager,
    hotkey: Option<HotKey>,
    quick_capture_window: Option<WindowHandle<QuickCaptureView>>,
    quick_capture_hotkey: Option<HotKey>,
    note_created: NoteCreatedHandler,
    _tray_icon: TrayIcon,
    open_menu_id: MenuId,
    quit_menu_id: MenuId,
}

impl Global for TrayController {}

pub fn init(
    window_handle: AnyWindowHandle,
    note_created: NoteCreatedHandler,
    cx: &mut App,
) -> Result<()> {
    let menu = Menu::new();
    let open_item = MenuItem::new("Open Castle", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    menu.append_items(&[&open_item, &quit_item])?;

    let tray_icon = TrayIconBuilder::new()
        .with_tooltip("Castle")
        .with_icon(castle_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?;

    let hotkey: HotKey = AppSettings::tray_shortcut(cx)
        .as_ref()
        .parse()
        .context("invalid tray shortcut")?;

    let hotkey_manager = GlobalHotKeyManager::new()?;
    let hotkey = match hotkey_manager.register(hotkey) {
        Ok(()) => Some(hotkey),
        Err(err) => {
            eprintln!("Failed to register global shortcut: {err}");
            None
        }
    };

    let quick_capture_hotkey: HotKey = AppSettings::quick_capture_shortcut(cx)
        .as_ref()
        .parse()
        .context("invalid quick capture shortcut")?;

    let quick_capture_hotkey = match hotkey_manager.register(quick_capture_hotkey) {
        Ok(()) => Some(quick_capture_hotkey),
        Err(err) => {
            eprintln!("Failed to register quick capture shortcut: {err}");
            None
        }
    };

    let set_window_visible: WindowVisibilityHandler = Rc::new(set_window_visible);
    let quick_capture_window =
        quick_capture::open_window(note_created.clone(), set_window_visible, cx)
            .context("failed to prewarm quick capture window")?;

    window_handle.update(cx, |_, window, cx| {
        window.on_window_should_close(cx, |window, cx| {
            if !AppSettings::close_to_tray(cx) {
                return true;
            }

            hide_window(window, cx);
            false
        });
    })?;

    cx.set_global(TrayController {
        window: window_handle,
        hotkey_manager,
        hotkey,
        quick_capture_window: Some(quick_capture_window),
        quick_capture_hotkey,
        note_created,
        _tray_icon: tray_icon,
        open_menu_id: open_item.id().clone(),
        quit_menu_id: quit_item.id().clone(),
    });

    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            cx.update(poll_events);
        }
    })
    .detach();

    Ok(())
}

pub fn update_shortcut(shortcut: &str, cx: &mut App) {
    if !cx.has_global::<TrayController>() {
        return;
    }

    let controller = cx.global_mut::<TrayController>();
    replace_hotkey(
        &controller.hotkey_manager,
        &mut controller.hotkey,
        shortcut,
        "global shortcut",
    );
}

pub fn update_quick_capture_shortcut(shortcut: &str, cx: &mut App) {
    if !cx.has_global::<TrayController>() {
        return;
    }

    let controller = cx.global_mut::<TrayController>();
    replace_hotkey(
        &controller.hotkey_manager,
        &mut controller.quick_capture_hotkey,
        shortcut,
        "quick capture shortcut",
    );
}

fn replace_hotkey(
    hotkey_manager: &GlobalHotKeyManager,
    current_hotkey: &mut Option<HotKey>,
    shortcut: &str,
    label: &str,
) {
    let Ok(new_hotkey) = shortcut.parse::<HotKey>() else {
        return;
    };

    if *current_hotkey == Some(new_hotkey) {
        return;
    }

    let previous_hotkey = *current_hotkey;
    if let Some(hotkey) = previous_hotkey {
        if let Err(err) = hotkey_manager.unregister(hotkey) {
            eprintln!("Failed to unregister {label}: {err}");
            return;
        }
        *current_hotkey = None;
    }

    if let Err(err) = hotkey_manager.register(new_hotkey) {
        eprintln!("Failed to register {label} {shortcut}: {err}");
        if let Some(hotkey) = previous_hotkey {
            if let Err(restore_err) = hotkey_manager.register(hotkey) {
                eprintln!("Failed to restore previous {label}: {restore_err}");
            } else {
                *current_hotkey = Some(hotkey);
            }
        }
        return;
    }

    *current_hotkey = Some(new_hotkey);
}

fn poll_events(cx: &mut App) {
    let (
        window,
        hotkey_id,
        quick_capture_window,
        quick_capture_hotkey_id,
        note_created,
        open_menu_id,
        quit_menu_id,
    ) = {
        let controller = cx.global::<TrayController>();
        (
            controller.window,
            controller.hotkey.map(|hotkey| hotkey.id()),
            controller.quick_capture_window,
            controller.quick_capture_hotkey.map(|hotkey| hotkey.id()),
            controller.note_created.clone(),
            controller.open_menu_id.clone(),
            controller.quit_menu_id.clone(),
        )
    };

    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
        if Some(event.id) == hotkey_id && event.state == HotKeyState::Pressed {
            show_window(window, cx);
        } else if Some(event.id) == quick_capture_hotkey_id && event.state == HotKeyState::Pressed {
            show_quick_capture(quick_capture_window, note_created.clone(), cx);
        }
    }

    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        ) {
            show_window(window, cx);
        }
    }

    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id == open_menu_id {
            show_window(window, cx);
        } else if event.id == quit_menu_id {
            cx.quit();
        }
    }
}

fn show_window(window_handle: AnyWindowHandle, cx: &mut App) {
    if let Err(err) = window_handle.update(cx, |_, window, cx| {
        set_window_visible(window, true);
        cx.activate(true);
        window.activate_window();
    }) {
        eprintln!("Failed to restore Castle window: {err}");
    }
}

fn show_quick_capture(
    window_handle: Option<WindowHandle<QuickCaptureView>>,
    note_created: NoteCreatedHandler,
    cx: &mut App,
) {
    if let Some(window_handle) = window_handle {
        match window_handle.update(cx, |view, window, cx| {
            set_window_visible(window, true);
            view.present(window, cx);
        }) {
            Ok(()) => return,
            Err(err) => eprintln!("Failed to show quick capture window: {err}"),
        }
    }

    let set_window_visible: WindowVisibilityHandler = Rc::new(set_window_visible);
    let visibility_for_window = set_window_visible.clone();
    let Ok(window_handle) = quick_capture::open_window(note_created, set_window_visible, cx) else {
        eprintln!("Failed to create quick capture window");
        return;
    };

    if let Err(err) = window_handle.update(cx, |view, window, cx| {
        visibility_for_window(window, true);
        view.present(window, cx);
    }) {
        eprintln!("Failed to present quick capture window: {err}");
        return;
    }

    cx.global_mut::<TrayController>().quick_capture_window = Some(window_handle);
}

fn hide_window(window: &Window, cx: &App) {
    set_window_visible(window, false);
    cx.hide();
}

#[cfg(target_os = "windows")]
pub fn set_window_visible(window: &Window, visible: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_RESTORE, ShowWindow};

    let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let command = if visible { SW_RESTORE } else { SW_HIDE };
    unsafe {
        ShowWindow(handle.hwnd.get() as *mut _, command);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_window_visible(_window: &Window, _visible: bool) {}

fn castle_icon() -> Result<Icon> {
    let image = image::load_from_memory(include_bytes!("../assets/icon/castle-tray.png"))
        .context("failed to decode Castle tray icon")?
        .into_rgba8();
    let (width, height) = image.dimensions();

    Icon::from_rgba(image.into_raw(), width, height).context("invalid tray icon")
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_tray_icon_decodes() {
        if let Err(err) = super::castle_icon() {
            panic!("bundled tray icon should be valid: {err}");
        }
    }
}
