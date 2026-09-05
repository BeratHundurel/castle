use gpui_kit::component::{
    ActiveTheme, Disableable as _, Icon, IconName, IndexPath, Sizable as _, Size, ThemeRegistry,
    WindowExt as _,
    button::Button,
    group_box::GroupBoxVariant,
    kbd::Kbd,
    notification::Notification,
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectEvent, SelectState},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
};
use gpui_kit::{
    App, AppContext as _, Axis, Context, Entity, IntoElement, Keystroke, ParentElement,
    SharedString, StyleRefinement, Styled, Subscription, Window, div, prelude::FluentBuilder as _,
    px, rems,
};

use std::{rc::Rc, sync::Arc};

use crate::{
    AppSettings, DEFAULT_QUICK_CAPTURE_SHORTCUT, DEFAULT_TRAY_SHORTCUT, scrollbar_show_key,
};

const SETTINGS_DIALOG_WIDTH: f32 = 960.0;
const SETTINGS_DIALOG_HEIGHT: f32 = 640.0;
const SETTINGS_DIALOG_MARGIN: f32 = 32.0;
const SETTINGS_DIALOG_MIN_WIDTH: f32 = 640.0;
const SETTINGS_DIALOG_MIN_HEIGHT: f32 = 360.0;
const SETTINGS_SIDEBAR_WIDE_WIDTH: f32 = 272.0;
const SETTINGS_SIDEBAR_MEDIUM_WIDTH: f32 = 240.0;
const SETTINGS_SIDEBAR_NARROW_WIDTH: f32 = 208.0;
const SETTINGS_SIDEBAR_HORIZONTAL_PADDING: f32 = 24.0;
const SETTINGS_PICKER_WIDTH: f32 = 360.0;
const THEME_SEARCH_PLACEHOLDER: &str = "Search themes...";
const FONT_SEARCH_PLACEHOLDER: &str = "Search fonts...";

#[derive(Clone, Debug, PartialEq, Eq)]
struct PickerOption {
    value: SharedString,
    label: SharedString,
}

impl PickerOption {
    fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

impl SearchableListItem for PickerOption {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        self.label.to_lowercase().contains(&query) || self.value.to_lowercase().contains(&query)
    }
}

type PickerSelectState = SelectState<SearchableVec<PickerOption>>;

struct SearchablePickerState {
    select: Entity<PickerSelectState>,
    _subscription: Subscription,
}

#[derive(Clone)]
pub struct ShortcutReference {
    pub action: SharedString,
    pub context: SharedString,
    pub keystrokes: Vec<Keystroke>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAccessAvailability {
    Enabled,
    Available,
    ServerUnavailable,
}

pub trait AgentAccess: Send + Sync {
    fn status(&self) -> Result<AgentAccessAvailability, String>;
    fn set_enabled(&self, enabled: bool) -> Result<AgentAccessAvailability, String>;
}

type SidebarVisible = Rc<dyn Fn(&App) -> bool>;
type SetSidebarVisible = Rc<dyn Fn(bool, &mut App)>;
type UpdateTrayShortcut = Rc<dyn Fn(&str, &mut App)>;
type UpdateQuickCaptureShortcut = Rc<dyn Fn(&str, &mut App)>;
type ShortcutProvider = Rc<dyn Fn(&App) -> Vec<ShortcutReference>>;
type WorkspaceArchiveAction = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct WorkspaceArchiveActions {
    import: WorkspaceArchiveAction,
    export: WorkspaceArchiveAction,
}

impl WorkspaceArchiveActions {
    pub fn new(
        import: impl Fn(&mut Window, &mut App) + 'static,
        export: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            import: Rc::new(import),
            export: Rc::new(export),
        }
    }
}

#[derive(Clone)]
pub struct SettingsIntegration {
    sidebar_visible: SidebarVisible,
    set_sidebar_visible: SetSidebarVisible,
    update_tray_shortcut: UpdateTrayShortcut,
    update_quick_capture_shortcut: UpdateQuickCaptureShortcut,
    shortcuts: ShortcutProvider,
    import_workspace: WorkspaceArchiveAction,
    export_workspace: WorkspaceArchiveAction,
    agent_access: Arc<dyn AgentAccess>,
}

impl SettingsIntegration {
    pub fn new(
        sidebar_visible: impl Fn(&App) -> bool + 'static,
        set_sidebar_visible: impl Fn(bool, &mut App) + 'static,
        update_tray_shortcut: impl Fn(&str, &mut App) + 'static,
        update_quick_capture_shortcut: impl Fn(&str, &mut App) + 'static,
        shortcuts: impl Fn(&App) -> Vec<ShortcutReference> + 'static,
        archive_actions: WorkspaceArchiveActions,
        agent_access: Arc<dyn AgentAccess>,
    ) -> Self {
        Self {
            sidebar_visible: Rc::new(sidebar_visible),
            set_sidebar_visible: Rc::new(set_sidebar_visible),
            update_tray_shortcut: Rc::new(update_tray_shortcut),
            update_quick_capture_shortcut: Rc::new(update_quick_capture_shortcut),
            shortcuts: Rc::new(shortcuts),
            import_workspace: archive_actions.import,
            export_workspace: archive_actions.export,
            agent_access,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AgentAccessState {
    Checking,
    Enabling,
    Disabling,
    Enabled,
    Available,
    ServerUnavailable,
    Error(SharedString),
}

pub struct SettingsView {
    dialog_open: bool,
    agent_access_state: AgentAccessState,
    integration: SettingsIntegration,
}

impl SettingsView {
    pub fn new(integration: SettingsIntegration) -> Self {
        Self {
            dialog_open: false,
            agent_access_state: AgentAccessState::Checking,
            integration,
        }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open {
            if window.has_active_dialog(cx) {
                return;
            }
            self.dialog_open = false;
        }

        self.dialog_open = true;
        let settings = cx.entity();
        let settings_owner = settings.clone();

        window.open_dialog(cx, move |dialog, window, _cx| {
            let dialog_width = responsive_dialog_dimension(
                window.viewport_size().width.as_f32(),
                SETTINGS_DIALOG_WIDTH,
                SETTINGS_DIALOG_MIN_WIDTH,
            );
            let dialog_height = responsive_dialog_dimension(
                window.viewport_size().height.as_f32(),
                SETTINGS_DIALOG_HEIGHT,
                SETTINGS_DIALOG_MIN_HEIGHT,
            );
            let sidebar_width = settings_sidebar_width(dialog_width);

            dialog
                .w(px(dialog_width))
                .h(px(dialog_height))
                .title("Settings")
                .on_close({
                    let settings_owner = settings_owner.clone();
                    move |_, _, cx| {
                        settings_owner.update(cx, |settings, cx| {
                            settings.dialog_open = false;
                            cx.notify();
                        });
                    }
                })
                .content({
                    let settings = settings.clone();
                    move |content, _, cx| {
                        content.px_4().pb_4().child(
                            div().size_full().overflow_hidden().child(
                                Settings::new("castle-settings")
                                    .with_size(Size::Medium)
                                    .with_group_variant(GroupBoxVariant::Fill)
                                    .sidebar_width(px(sidebar_width))
                                    .sidebar_style(&settings_sidebar_style(
                                        *cx.theme().tokens.background,
                                        cx.theme().border,
                                    ))
                                    .header_style(&settings_header_style(sidebar_width))
                                    .pages(setting_pages(settings.clone(), cx)),
                            ),
                        )
                    }
                })
        });
    }

    pub fn refresh_agent_access(&mut self, cx: &mut Context<Self>) {
        self.agent_access_state = AgentAccessState::Checking;
        let agent_access = self.integration.agent_access.clone();
        let status = cx
            .background_executor()
            .spawn(async move { agent_access.status() });

        cx.spawn(async move |this, cx| {
            let next_state = match status.await {
                Ok(AgentAccessAvailability::Enabled) => AgentAccessState::Enabled,
                Ok(AgentAccessAvailability::Available) => AgentAccessState::Available,
                Ok(AgentAccessAvailability::ServerUnavailable) => {
                    AgentAccessState::ServerUnavailable
                }
                Err(error) => AgentAccessState::Error(error.into()),
            };
            this.update(cx, |this, cx| {
                this.agent_access_state = next_state;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn set_agent_access_enabled(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_access_state = if enabled {
            AgentAccessState::Enabling
        } else {
            AgentAccessState::Disabling
        };
        cx.notify();

        let agent_access = self.integration.agent_access.clone();
        let operation = cx
            .background_executor()
            .spawn(async move { agent_access.set_enabled(enabled) });
        cx.spawn_in(window, async move |this, cx| {
            let result = operation.await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(status) => {
                        this.agent_access_state = match status {
                            AgentAccessAvailability::Enabled => AgentAccessState::Enabled,
                            AgentAccessAvailability::Available => AgentAccessState::Available,
                            AgentAccessAvailability::ServerUnavailable => {
                                AgentAccessState::ServerUnavailable
                            }
                        };
                        let message = if enabled {
                            "MCP enabled. Restart open Codex clients to connect Castle."
                        } else {
                            "MCP disabled. Restart open Codex clients to disconnect Castle."
                        };
                        window.push_notification(Notification::success(message), cx);
                    }
                    Err(error) => {
                        let message = error;
                        this.agent_access_state = AgentAccessState::Error(message.clone().into());
                        window.push_notification(
                            Notification::error(format!("Could not update MCP setup: {message}")),
                            cx,
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn responsive_dialog_dimension(available: f32, preferred: f32, minimum: f32) -> f32 {
    let max = (available - SETTINGS_DIALOG_MARGIN * 2.0).max(1.0);

    if max >= minimum {
        preferred.min(max).max(minimum)
    } else {
        max
    }
}

fn settings_sidebar_width(dialog_width: f32) -> f32 {
    if dialog_width < 760.0 {
        SETTINGS_SIDEBAR_NARROW_WIDTH
    } else if dialog_width < 900.0 {
        SETTINGS_SIDEBAR_MEDIUM_WIDTH
    } else {
        SETTINGS_SIDEBAR_WIDE_WIDTH
    }
}

fn settings_header_style(sidebar_width: f32) -> StyleRefinement {
    let search_width = sidebar_width - SETTINGS_SIDEBAR_HORIZONTAL_PADDING;

    StyleRefinement::default().w(px(search_width)).max_w_full()
}

fn settings_sidebar_style(background: gpui_kit::Hsla, border: gpui_kit::Hsla) -> StyleRefinement {
    StyleRefinement::default()
        .bg(background)
        .border_color(border)
}

fn setting_pages(settings: Entity<SettingsView>, cx: &mut App) -> Vec<SettingPage> {
    vec![
        SettingPage::new("General")
            .default_open(true)
            .icon(Icon::new(IconName::Settings2))
            .groups(vec![
                SettingGroup::new().title("Appearance").items(vec![
                    SettingItem::new(
                        "Theme",
                        searchable_select_field(
                            "theme",
                            THEME_SEARCH_PLACEHOLDER,
                            theme_options(cx),
                            current_theme_name,
                            AppSettings::set_theme_name,
                        ),
                    )
                    .description("Choose the color theme used across Castle.")
                    .layout(Axis::Vertical),
                    SettingItem::new(
                        "Interface Font",
                        searchable_select_field(
                            "interface-font",
                            FONT_SEARCH_PLACEHOLDER,
                            font_options(cx),
                            AppSettings::font_family,
                            AppSettings::set_font_family,
                        ),
                    )
                    .description("Choose the font family used across the interface.")
                    .layout(Axis::Vertical),
                    SettingItem::new(
                        "Font Size",
                        SettingField::number_input(
                            NumberFieldOptions {
                                min: 12.0,
                                max: 20.0,
                                step: 1.0,
                            },
                            |cx: &App| cx.theme().font_size.as_f32() as f64,
                            |font_size: f64, cx: &mut App| {
                                AppSettings::set_font_size(font_size, cx);
                            },
                        )
                        .default_value(16.0),
                    )
                    .description("Adjust the base UI text size."),
                    SettingItem::new(
                        "Corner Radius",
                        SettingField::number_input(
                            NumberFieldOptions {
                                min: 0.0,
                                max: 12.0,
                                step: 1.0,
                            },
                            |cx: &App| cx.theme().radius.as_f32() as f64,
                            |radius: f64, cx: &mut App| {
                                AppSettings::set_radius(radius, cx);
                            },
                        )
                        .default_value(6.0),
                    )
                    .description("Control how rounded buttons, panels, and inputs appear."),
                ]),
                SettingGroup::new().title("Layout").items(vec![
                    SettingItem::new(
                        "Show Sidebar",
                        SettingField::switch(
                            {
                                let settings = settings.clone();
                                move |cx: &App| {
                                    let sidebar_visible =
                                        settings.read(cx).integration.sidebar_visible.clone();
                                    sidebar_visible(cx)
                                }
                            },
                            {
                                let settings = settings.clone();
                                move |visible: bool, cx: &mut App| {
                                    let set_sidebar_visible =
                                        settings.read(cx).integration.set_sidebar_visible.clone();
                                    set_sidebar_visible(visible, cx);
                                }
                            },
                        )
                        .default_value(true),
                    )
                    .description("Keep the project and workspace navigation visible."),
                    SettingItem::new(
                        "Scrollbars",
                        SettingField::dropdown(
                            vec![
                                ("scrolling".into(), "During scroll".into()),
                                ("hover".into(), "On hover".into()),
                                ("always".into(), "Always".into()),
                            ],
                            |cx: &App| scrollbar_show_key(cx.theme().scrollbar_mode),
                            |value: SharedString, cx: &mut App| {
                                AppSettings::set_scrollbar_show(value, cx);
                            },
                        )
                        .default_value("scrolling"),
                    )
                    .description("Choose when scrollbars are shown in long lists and editors."),
                ]),
                SettingGroup::new().title("Tray").items(vec![
                    SettingItem::new(
                        "Close to Tray",
                        SettingField::switch(
                            AppSettings::close_to_tray,
                            AppSettings::set_close_to_tray,
                        )
                        .default_value(true),
                    )
                    .description(
                        "Keep Castle running in the system tray when its window is closed.",
                    ),
                    SettingItem::new(
                        "Open Shortcut",
                        SettingField::input(AppSettings::tray_shortcut, {
                            let settings = settings.clone();
                            move |shortcut, cx| {
                                AppSettings::set_tray_shortcut(shortcut.clone(), cx);
                                let update_tray_shortcut =
                                    settings.read(cx).integration.update_tray_shortcut.clone();
                                update_tray_shortcut(shortcut.as_ref(), cx);
                            }
                        })
                        .default_value(DEFAULT_TRAY_SHORTCUT),
                    )
                    .description(
                        "Global shortcut used to restore Castle, for example Ctrl+Alt+Space.",
                    )
                    .layout(Axis::Vertical),
                    SettingItem::new(
                        "Quick Capture Shortcut",
                        SettingField::input(AppSettings::quick_capture_shortcut, {
                            let settings = settings.clone();
                            move |shortcut, cx| {
                                AppSettings::set_quick_capture_shortcut(shortcut.clone(), cx);
                                let update_quick_capture_shortcut = settings
                                    .read(cx)
                                    .integration
                                    .update_quick_capture_shortcut
                                    .clone();
                                update_quick_capture_shortcut(shortcut.as_ref(), cx);
                            }
                        })
                        .default_value(DEFAULT_QUICK_CAPTURE_SHORTCUT),
                    )
                    .description(
                        "Global shortcut that opens a focused note capture window, for example Ctrl+Alt+N.",
                    )
                    .layout(Axis::Vertical),
                ]),
                SettingGroup::new()
                    .title("Workspace")
                    .item(workspace_archive_item(settings.clone())),
            ]),
        SettingPage::new("Editor")
            .icon(Icon::new(IconName::BookOpen))
            .group(SettingGroup::new().title("Documents").items(vec![
                SettingItem::new(
                    "Editor Font",
                    searchable_select_field(
                        "editor-font",
                        FONT_SEARCH_PLACEHOLDER,
                        font_options(cx),
                        AppSettings::editor_font_family,
                        AppSettings::set_editor_font_family,
                    ),
                )
                .description("Choose the monospace font family used while editing documents.")
                .layout(Axis::Vertical),
                SettingItem::new(
                    "Editor Font Size",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 10.0,
                            max: 22.0,
                            step: 1.0,
                        },
                        AppSettings::editor_font_size,
                        AppSettings::set_editor_font_size,
                    )
                    .default_value(13.0),
                )
                .description("Adjust the monospace font size used while editing documents."),
                SettingItem::new(
                    "Status Line",
                    SettingField::switch(
                        AppSettings::editor_status_line_visible,
                        AppSettings::set_editor_status_line_visible,
                    )
                    .default_value(true),
                )
                .description(
                    "Show file, document type, save state, and statistics below document editors.",
                ),
                SettingItem::new(
                    "Line Numbers",
                    SettingField::switch(
                        AppSettings::editor_line_numbers,
                        AppSettings::set_editor_line_numbers,
                    )
                    .default_value(false),
                )
                .description("Show line numbers in newly opened document editors."),
                SettingItem::new(
                    "Soft Wrap",
                    SettingField::switch(
                        AppSettings::editor_soft_wrap,
                        AppSettings::set_editor_soft_wrap,
                    )
                    .default_value(true),
                )
                .description("Wrap long lines in newly opened document editors."),
                SettingItem::new(
                    "Focus Mode",
                    SettingField::switch(
                        AppSettings::editor_focus_mode,
                        AppSettings::set_editor_focus_mode,
                    )
                    .default_value(false),
                )
                .description("Dim text outside the active paragraph in document source editors."),
                SettingItem::new(
                    "Typewriter Scrolling",
                    SettingField::switch(
                        AppSettings::editor_typewriter_scrolling,
                        AppSettings::set_editor_typewriter_scrolling,
                    )
                    .default_value(false),
                )
                .description("Keep the active line near the vertical center while writing."),
                SettingItem::new(
                    "Vim Mode",
                    SettingField::switch(
                        AppSettings::editor_vim_mode,
                        AppSettings::set_editor_vim_mode,
                    )
                    .default_value(false),
                )
                .description(
                    "Use opt-in Normal, Insert, and Visual modes in document source editors.",
                ),
            ]))
            .group(SettingGroup::new().title("Markdown").items(vec![
                SettingItem::new(
                    "Preview Font Size",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 10.0,
                            max: 22.0,
                            step: 1.0,
                        },
                        AppSettings::markdown_preview_font_size,
                        AppSettings::set_markdown_preview_font_size,
                    )
                    .default_value(16.0),
                )
                .description("Adjust the font size used while reading rendered Markdown."),
                SettingItem::new(
                    "Default Note View",
                    SettingField::dropdown(
                        vec![
                            ("source".into(), "Write".into()),
                            ("split".into(), "Side by side".into()),
                            ("preview".into(), "Read".into()),
                        ],
                        AppSettings::markdown_editor_mode,
                        AppSettings::set_markdown_editor_mode,
                    )
                    .default_value("source"),
                )
                .description("Choose the view used when a Markdown note opens."),
            ])),
        SettingPage::new("Shortcuts")
            .icon(Icon::new(IconName::SquareTerminal))
            .description("Keyboard shortcuts currently registered by Castle.")
            .resettable(false)
            .groups(shortcut_groups(&settings, cx)),
        SettingPage::new("Agent Access")
            .icon(Icon::new(IconName::SquareTerminal))
            .description("Let trusted local agents work with your Castle notes and boards.")
            .resettable(false)
            .group(
                SettingGroup::new()
                    .title("Model Context Protocol")
                    .item(agent_access_item(settings.clone())),
            ),
        SettingPage::new("About")
            .icon(Icon::new(IconName::Info))
            .group(
                SettingGroup::new()
                    .title("Castle")
                    .items(vec![SettingItem::render(|options, _, cx| {
                        gpui_kit::component::h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                gpui_kit::component::v_flex().gap_1().child("Castle").child(
                                    gpui_kit::div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("A private notes and kanban workspace."),
                                ),
                            )
                            .child(
                                gpui_kit::component::button::Button::new("settings-about-version")
                                    .label(env!("CARGO_PKG_VERSION"))
                                    .outline()
                                    .with_size(options.size())
                                    .tab_stop(false),
                            )
                            .into_any_element()
                    })]),
            ),
    ]
}

fn agent_access_item(settings: Entity<SettingsView>) -> SettingItem {
    SettingItem::render(move |options, _window, cx| {
        let state = settings.read(cx).agent_access_state.clone();
        let (status, label, disabled, enable) = match &state {
            AgentAccessState::Checking => ("Checking setup…", "Checking…", true, true),
            AgentAccessState::Enabling => ("Adding Castle to Codex…", "Enabling…", true, true),
            AgentAccessState::Disabling => {
                ("Removing Castle from Codex…", "Disabling…", true, false)
            }
            AgentAccessState::Enabled => (
                "Enabled for Codex clients on this computer.",
                "Disable MCP",
                false,
                false,
            ),
            AgentAccessState::Available => (
                "Ready to enable. No agent configuration has been changed.",
                "Enable MCP",
                false,
                true,
            ),
            AgentAccessState::ServerUnavailable => (
                "Place the matching Castle-MCP executable beside Castle to enable agent access.",
                "Helper not found",
                true,
                true,
            ),
            AgentAccessState::Error(message) => (message.as_ref(), "Try again", false, true),
        };
        let stacked = settings_row_is_stacked(options.layout());

        gpui_kit::component::h_flex()
            .w_full()
            .gap_4()
            .when(stacked, |this| this.flex_col().items_start())
            .when(!stacked, |this| this.items_center().justify_between())
            .child(
                gpui_kit::component::v_flex()
                    .min_w_0()
                    .when(!stacked, |this| this.flex_1())
                    .when(stacked, |this| this.w_full())
                    .gap_1()
                    .child("Castle MCP")
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status.to_string()),
                    ),
            )
            .child(
                Button::new("settings-mcp-toggle")
                    .label(label)
                    .outline()
                    .with_size(options.size())
                    .disabled(disabled)
                    .on_click({
                        let settings = settings.clone();
                        move |_, window, cx| {
                            settings.update(cx, |settings, cx| {
                                settings.set_agent_access_enabled(enable, window, cx);
                            });
                        }
                    }),
            )
            .into_any_element()
    })
}

fn workspace_archive_item(settings: Entity<SettingsView>) -> SettingItem {
    SettingItem::render(move |options, _, cx| {
        let integration = settings.read(cx).integration.clone();
        let import_workspace = integration.import_workspace.clone();
        let export_workspace = integration.export_workspace.clone();
        let stacked = settings_row_is_stacked(options.layout());

        gpui_kit::component::h_flex()
            .w_full()
            .gap_4()
            .when(stacked, |this| this.flex_col().items_start())
            .when(!stacked, |this| this.items_center().justify_between())
            .child(
                gpui_kit::component::v_flex()
                    .min_w_0()
                    .when(!stacked, |this| this.flex_1())
                    .when(stacked, |this| this.w_full())
                    .gap_1()
                    .child("Workspace archive")
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Move notes, boards, links, attachments, and settings between Castle installations."),
                    ),
            )
            .child(
                gpui_kit::component::h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        Button::new("settings-import-workspace")
                            .icon(IconName::FolderOpen)
                            .label("Import")
                            .outline()
                            .with_size(options.size())
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                import_workspace(window, cx);
                            }),
                    )
                    .child(
                        Button::new("settings-export-workspace")
                            .icon(IconName::Folder)
                            .label("Export")
                            .outline()
                            .with_size(options.size())
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                export_workspace(window, cx);
                            }),
                    ),
            )
            .into_any_element()
    })
}

fn shortcut_groups(settings: &Entity<SettingsView>, cx: &App) -> Vec<SettingGroup> {
    let mut contexts = std::collections::BTreeMap::<SharedString, Vec<_>>::new();

    let shortcuts = settings.read(cx).integration.shortcuts.clone();
    for shortcut in shortcuts(cx) {
        contexts
            .entry(shortcut.context.clone())
            .or_default()
            .push(shortcut);
    }

    contexts
        .into_iter()
        .map(|(context, mut shortcuts)| {
            shortcuts.sort_by(|left, right| left.action.cmp(&right.action));
            SettingGroup::new()
                .title(shortcut_context_name(&context))
                .items(shortcuts.into_iter().map(|shortcut| {
                    SettingItem::render(move |_, _, cx| {
                        gpui_kit::component::h_flex()
                            .w_full()
                            .min_h(rems(2.25))
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child(shortcut.action.clone()),
                            )
                            .child(
                                gpui_kit::component::h_flex()
                                    .flex_shrink_0()
                                    .gap_1()
                                    .children(
                                        shortcut
                                            .keystrokes
                                            .iter()
                                            .cloned()
                                            .map(|stroke| Kbd::new(stroke).outline()),
                                    ),
                            )
                    })
                }))
        })
        .collect()
}

fn settings_row_is_stacked(layout: Axis) -> bool {
    layout == Axis::Vertical
}

fn shortcut_context_name(context: &str) -> SharedString {
    match context {
        "AppShell" => "Application".into(),
        "CommandPalette" => "Command Palette".into(),
        "DocumentEditor" => "Document Editor".into(),
        "DocumentOutline" => "Document Outline".into(),
        "EmmetInput" => "Emmet Input".into(),
        "TextView" => "Text View".into(),
        _ => humanize_identifier(context),
    }
}

fn humanize_identifier(value: &str) -> SharedString {
    let mut label = String::with_capacity(value.len() + 4);
    let mut previous_is_lowercase = false;

    for character in value.chars() {
        if character == '_' || character == '-' {
            if !label.ends_with(' ') {
                label.push(' ');
            }
            previous_is_lowercase = false;
            continue;
        }

        if character.is_uppercase() && previous_is_lowercase {
            label.push(' ');
        }
        label.push(character);
        previous_is_lowercase = character.is_lowercase();
    }

    label.into()
}

fn current_theme_name(cx: &App) -> SharedString {
    cx.theme().theme_name().clone()
}

fn theme_options(cx: &App) -> Vec<PickerOption> {
    ThemeRegistry::global(cx)
        .sorted_themes()
        .iter()
        .map(|theme| PickerOption::new(theme.name.clone(), theme.name.clone()))
        .collect()
}

fn font_options(cx: &App) -> Vec<PickerOption> {
    cx.text_system()
        .all_font_names()
        .into_iter()
        .map(|font_name| {
            let label = if font_name == ".SystemUIFont" {
                "System UI".to_string()
            } else {
                font_name.clone()
            };

            PickerOption::new(font_name, label)
        })
        .collect()
}

fn searchable_select_field(
    id: &'static str,
    search_placeholder: &'static str,
    options: Vec<PickerOption>,
    value: fn(&App) -> SharedString,
    set_value: fn(SharedString, &mut App),
) -> SettingField<SharedString> {
    SettingField::render(move |render_options, window: &mut Window, cx: &mut App| {
        let selected_value = value(cx);
        let picker_options = with_selected_option(options.clone(), selected_value.clone());
        let selected_index = selected_index(&picker_options, &selected_value);
        let state_key = SharedString::from(format!(
            "settings-{id}-{}-{}-{}",
            render_options.page_ix(),
            render_options.group_ix(),
            render_options.item_ix()
        ));

        let state = window.use_keyed_state(state_key, cx, |window, cx| {
            let initial_options = picker_options.clone();
            let select = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(initial_options),
                    selected_index,
                    window,
                    cx,
                )
                .searchable(true)
            });
            let _subscription = cx.subscribe(&select, move |_, _, event, cx| {
                let SelectEvent::Confirm(next_value) = event;
                if let Some(next_value) = next_value {
                    set_value(next_value.clone(), cx);
                }
            });

            SearchablePickerState {
                select,
                _subscription,
            }
        });

        let select = state.read(cx).select.clone();

        div().w(px(SETTINGS_PICKER_WIDTH)).max_w_full().child(
            Select::new(&select)
                .with_size(render_options.size())
                .search_placeholder(search_placeholder)
                .menu_max_h(rems(18.))
                .w_full(),
        )
    })
}

fn selected_index(options: &[PickerOption], selected_value: &SharedString) -> Option<IndexPath> {
    options
        .iter()
        .position(|option| &option.value == selected_value)
        .map(|index| IndexPath::default().row(index))
}

fn with_selected_option(
    mut options: Vec<PickerOption>,
    selected_value: SharedString,
) -> Vec<PickerOption> {
    if !selected_value.is_empty() && !options.iter().any(|option| option.value == selected_value) {
        options.push(PickerOption::new(
            selected_value.clone(),
            selected_value.clone(),
        ));
    }

    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_sidebar_uses_the_dialog_background() {
        let background = gpui_kit::hsla(0.6, 0.2, 0.15, 1.0);
        let border = gpui_kit::hsla(0.6, 0.1, 0.3, 1.0);
        let style = settings_sidebar_style(background, border);

        assert_eq!(
            style.background.as_ref().and_then(|fill| fill.color()),
            Some(background.into())
        );
        assert_eq!(style.border_color, Some(border));
    }

    #[test]
    fn custom_setting_rows_follow_the_settings_stack_layout() {
        assert!(settings_row_is_stacked(Axis::Vertical));
        assert!(!settings_row_is_stacked(Axis::Horizontal));
    }
}
