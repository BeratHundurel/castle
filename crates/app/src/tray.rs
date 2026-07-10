use std::time::Duration;

use anyhow::{Context as _, Result};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use gpui::{AnyWindowHandle, App, Global, Window};
use raw_window_handle::RawWindowHandle;
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem},
};

use crate::app_settings::AppSettings;

struct TrayController {
    window: AnyWindowHandle,
    hotkey_manager: GlobalHotKeyManager,
    hotkey: Option<HotKey>,
    _tray_icon: TrayIcon,
    open_menu_id: MenuId,
    quit_menu_id: MenuId,
}

impl Global for TrayController {}

pub fn init(window_handle: AnyWindowHandle, cx: &mut App) -> Result<()> {
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
        _tray_icon: tray_icon,
        open_menu_id: open_item.id().clone(),
        quit_menu_id: quit_item.id().clone(),
    });

    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            cx.update(poll_events);
        }
    })
    .detach();

    Ok(())
}

pub(crate) fn update_shortcut(shortcut: &str, cx: &mut App) {
    let Ok(new_hotkey) = shortcut.parse::<HotKey>() else {
        return;
    };

    if !cx.has_global::<TrayController>() {
        return;
    }

    let controller = cx.global_mut::<TrayController>();
    if controller.hotkey == Some(new_hotkey) {
        return;
    }

    if let Some(hotkey) = controller.hotkey
        && let Err(err) = controller.hotkey_manager.unregister(hotkey)
    {
        eprintln!("Failed to unregister global shortcut: {err}");
        return;
    }

    if let Err(err) = controller.hotkey_manager.register(new_hotkey) {
        eprintln!("Failed to register global shortcut {shortcut}: {err}");
        if let Some(hotkey) = controller.hotkey
            && let Err(restore_err) = controller.hotkey_manager.register(hotkey)
        {
            eprintln!("Failed to restore previous global shortcut: {restore_err}");
        }
        return;
    }

    controller.hotkey = Some(new_hotkey);
}

fn poll_events(cx: &mut App) {
    let (window, hotkey_id, open_menu_id, quit_menu_id) = {
        let controller = cx.global::<TrayController>();
        (
            controller.window,
            controller.hotkey.map(|hotkey| hotkey.id()),
            controller.open_menu_id.clone(),
            controller.quit_menu_id.clone(),
        )
    };

    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
        if Some(event.id) == hotkey_id && event.state == HotKeyState::Pressed {
            show_window(window, cx);
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

fn hide_window(window: &Window, cx: &App) {
    set_window_visible(window, false);
    cx.hide();
}

#[cfg(target_os = "windows")]
fn set_window_visible(window: &Window, visible: bool) {
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
fn set_window_visible(_window: &Window, _visible: bool) {}

fn castle_icon() -> Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
    for y in 5..28 {
        for x in 5..27 {
            let battlement_gap = y < 11 && matches!(x, 10..=12 | 19..=21);
            let outside_tower = y < 11 && !(x <= 8 || (14..=17).contains(&x) || x >= 23);
            let doorway = y >= 20 && (14..=17).contains(&x);
            if battlement_gap || outside_tower || doorway {
                continue;
            }

            let index = ((y * SIZE + x) * 4) as usize;
            rgba[index] = 224;
            rgba[index + 1] = 179;
            rgba[index + 2] = 84;
            rgba[index + 3] = 255;
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).context("invalid tray icon")
}
