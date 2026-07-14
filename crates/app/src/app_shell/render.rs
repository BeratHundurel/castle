use gpui::Hsla;
use gpui_component::Icon;

use crate::command_palette::{
    CloseCommandPaletteAction, CommandPaletteAction, OpenWorkspaceSearchAction,
    SelectNextCommandPaletteItem, SelectPrevCommandPaletteItem, SwitchThemeAction,
};

use super::*;

const SIDEBAR_WIDTH: f32 = 260.;
const COLLAPSED_TITLE_BAR_WIDTH: f32 = 48.;

impl AppShell {
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let sidebar_collapsed = self.sidebar.read(cx).is_collapsed();
        let sidebar_title_width = if sidebar_collapsed {
            COLLAPSED_TITLE_BAR_WIDTH
        } else {
            SIDEBAR_WIDTH
        };

        TitleBar::new().border_0().bg(theme.sidebar).child(
            h_flex()
                .id("title-bar-content")
                .size_full()
                .items_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, window, _| window.prevent_default())
                .child(
                    h_flex()
                        .id("sidebar-title-bar")
                        .w(px(sidebar_title_width))
                        .h_full()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .child(
                            SidebarToggleButton::new()
                                .collapsed(sidebar_collapsed)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let visible = this.sidebar.read(cx).is_collapsed();
                                    this.set_sidebar_visible(visible, cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(self.render_tabs(cx)),
                )
                .child(self.render_settings_button(cx))
                .child(self.render_note_save_controls(cx)),
        )
    }

    fn render_settings_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("title-bar-settings")
            .h_full()
            .items_center()
            .px_3()
            .flex_shrink_0()
            .child(
                Button::new("title-open-settings")
                    .icon(IconName::Settings2)
                    .ghost()
                    .xsmall()
                    .tooltip(format!("Settings ({})", settings_shortcut()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_settings(window, cx);
                    })),
            )
    }

    fn render_note_save_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(view) =
            self.open_tabs
                .get(self.active_tab_index)
                .and_then(|tab| match &tab.kind {
                    OpenTabKind::Note { view, .. } => Some(view.clone()),
                    _ => None,
                })
        else {
            return div().into_any_element();
        };

        let save_state = view.read(cx).save_state();
        let save_shortcut = platform_shortcut("S");
        let save_as_shortcut = platform_shortcut("Shift+S");
        let save_view = view.clone();
        let save_as_view = view;

        h_flex()
            .id("title-bar-note-actions")
            .h_full()
            .items_center()
            .gap_2()
            .px_3()
            .flex_shrink_0()
            .child(
                Button::new("title-save-note")
                    .icon(IconName::Check)
                    .ghost()
                    .xsmall()
                    .tooltip(format!("Save ({save_shortcut})"))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        save_view.update(cx, |note, cx| note.save(cx));
                    })),
            )
            .child(
                Button::new("title-save-note-as")
                    .icon(IconName::File)
                    .ghost()
                    .xsmall()
                    .tooltip(format!("Save as ({save_as_shortcut})"))
                    .on_click(cx.listener(move |_, _, window, cx| {
                        save_as_view.update(cx, |note, cx| note.save_as(window, cx));
                    })),
            )
            .child(save_status_pill(save_state, cx))
            .into_any_element()
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_index = self
            .active_tab_index
            .min(self.open_tabs.len().saturating_sub(1));

        let active_tab_id = self.open_tabs.get(active_index).map(|t| t.id);

        let tab_bar = TabBar::new("open-tabs")
            .pill()
            .small()
            .bg(cx.theme().sidebar)
            .selected_index(active_index)
            .on_click(cx.listener(|this, index: &usize, window, cx| {
                this.activate_tab(*index, window, cx);
            }))
            .last_empty_space(
                Button::new("new-tab")
                    .icon(IconName::Plus)
                    .ghost()
                    .xsmall()
                    .tooltip("New tab")
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx))),
            )
            .suffix(div().w_0())
            .children(self.open_tabs.iter().enumerate().map(|(index, tab)| {
                Tab::new()
                    .px_2()
                    .text_color(cx.theme().primary_foreground)
                    .label(tab_label(tab, cx))
                    .suffix(
                        Button::new(("close-tab", tab.id as usize))
                            .icon(IconName::Close)
                            .when_else(index == active_index, |b| b.primary(), |b| b.ghost())
                            .xsmall()
                            .tooltip("Close tab")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab(index, window, cx);
                            })),
                    )
            }));

        if let Some(tab_id) = active_tab_id {
            div()
                .child(tab_bar)
                .context_menu(move |menu, _, _cx| {
                    menu.menu("Close", Box::new(CloseTabAction(tab_id)))
                        .menu("Close Others", Box::new(CloseOtherTabsAction(tab_id)))
                        .menu("Close All", Box::new(CloseAllTabsAction))
                })
                .into_any_element()
        } else {
            tab_bar.into_any_element()
        }
    }

    fn render_active_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(tab) = self.open_tabs.get(self.active_tab_index) else {
            return div().size_full().into_any_element();
        };

        match &tab.kind {
            OpenTabKind::Chooser => self.render_chooser(cx).into_any_element(),
            OpenTabKind::Trash => self.render_trash(cx).into_any_element(),
            OpenTabKind::Board { view, .. } => view.clone().into_any_element(),
            OpenTabKind::Note { view, .. } => view.clone().into_any_element(),
        }
    }

    fn render_chooser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_home(cx)
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let theme = cx.theme().clone();

        v_flex()
            .id("app-container")
            .track_focus(&self.focus_handle)
            .key_context("AppShell")
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &CycleNextTab, window, cx| {
                this.cycle_next_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CyclePrevTab, window, cx| {
                this.cycle_prev_tab(window, cx);
            }))
            .on_action(cx.listener(|this, action: &CloseTabAction, window, cx| {
                this.close_tab_by_id(action.0, window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &CloseOtherTabsAction, window, cx| {
                    this.close_other_tabs(action.0, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &CloseAllTabsAction, window, cx| {
                this.close_all_tabs(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebarAction, window, cx| {
                this.on_toggle_sidebar_action(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSettingsAction, window, cx| {
                this.open_settings(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CommandPaletteAction, window, cx| {
                this.on_command_palette_action(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &OpenWorkspaceSearchAction, window, cx| {
                    this.open_workspace_search(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SwitchThemeAction, window, cx| {
                this.open_theme_switcher(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &CloseCommandPaletteAction, window, cx| {
                    this.on_close_command_palette_action(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                this.on_close_command_palette_action(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SelectPrevCommandPaletteItem, _, cx| {
                    this.select_prev_command_palette_item(cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &SelectNextCommandPaletteItem, _, cx| {
                    this.select_next_command_palette_item(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &InputMoveUp, _, cx| {
                this.select_prev_command_palette_item(cx);
            }))
            .on_action(cx.listener(|this, _: &InputMoveDown, _, cx| {
                this.select_next_command_palette_item(cx);
            }))
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .id("main-container")
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .child(self.sidebar.clone())
                    .child(
                        v_flex()
                            .id("content-container")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(self.render_active_tab(cx)),
                            ),
                    )
                    .when(self.command_palette.open, |this| {
                        this.child(self.render_command_palette_overlay(cx))
                    })
                    .children(dialog_layer),
            )
    }
}

fn tab_label(tab: &OpenTab, cx: &mut Context<AppShell>) -> SharedString {
    match &tab.kind {
        OpenTabKind::Note { view, .. } => {
            let state = view.read(cx).save_state();
            if matches!(
                state,
                SaveState::Dirty | SaveState::Missing | SaveState::Error(_)
            ) {
                SharedString::from(format!("* {}", tab.title))
            } else {
                tab.title.clone()
            }
        }
        OpenTabKind::Trash => tab.title.clone(),
        _ => tab.title.clone(),
    }
}

fn save_status_pill(save_state: SaveState, cx: &mut Context<AppShell>) -> impl IntoElement {
    let (icon, color, label) = save_state_status(save_state, cx);

    Button::new("title-save-status")
        .icon(Icon::new(icon).text_color(color))
        .ghost()
        .xsmall()
        .rounded_full()
        .bg(color.opacity(0.12))
        .tab_stop(false)
        .tooltip(label)
        .into_any_element()
}

fn save_state_status(
    save_state: SaveState,
    cx: &mut Context<AppShell>,
) -> (IconName, Hsla, &'static str) {
    match save_state {
        SaveState::Saved => (IconName::CircleCheck, cx.theme().success, "Saved"),
        SaveState::Dirty => (IconName::Asterisk, cx.theme().warning, "Unsaved changes"),
        SaveState::Saving => (IconName::Loader, cx.theme().info, "Saving"),
        SaveState::Missing => (IconName::TriangleAlert, cx.theme().warning, "File missing"),
        SaveState::Error(_) => (IconName::TriangleAlert, cx.theme().danger, "Save failed"),
    }
}

fn platform_shortcut(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("Cmd+{keys}")
    } else {
        format!("Ctrl+{keys}")
    }
}

fn settings_shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+,"
    } else {
        "Ctrl+,"
    }
}
