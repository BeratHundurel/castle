use gpui::{
    App, AppContext as _, Axis, Context, Entity, IntoElement, ParentElement, SharedString,
    StyleRefinement, Styled, Subscription, Window, div, px, rems,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, IndexPath, Sizable as _, Size, ThemeRegistry, WindowExt as _,
    group_box::GroupBoxVariant,
    kbd::Kbd,
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectEvent, SelectState},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
};

use crate::app_settings::{AppSettings, scrollbar_show_key};
use crate::keymap::{humanize_identifier, shortcuts};
use crate::markdown_editor::types::EditorMode;

use super::AppShell;

const SETTINGS_DIALOG_WIDTH: f32 = 960.0;
const SETTINGS_DIALOG_HEIGHT: f32 = 640.0;
const SETTINGS_DIALOG_MARGIN: f32 = 32.0;
const SETTINGS_DIALOG_MIN_WIDTH: f32 = 640.0;
const SETTINGS_DIALOG_MIN_HEIGHT: f32 = 360.0;
const SETTINGS_SIDEBAR_WIDE_WIDTH: f32 = 300.0;
const SETTINGS_SIDEBAR_MEDIUM_WIDTH: f32 = 260.0;
const SETTINGS_SIDEBAR_NARROW_WIDTH: f32 = 220.0;
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

impl AppShell {
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_dialog_open {
            if window.has_active_dialog(cx) {
                return;
            }
            self.settings_dialog_open = false;
        }

        self.settings_dialog_open = true;
        let app = cx.entity();
        let settings_owner = app.clone();

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
                        settings_owner.update(cx, |app, cx| {
                            app.settings_dialog_open = false;
                            cx.notify();
                        });
                    }
                })
                .content({
                    let app = app.clone();
                    move |content, _, cx| {
                        content.px_4().pb_4().child(
                            div().size_full().overflow_hidden().child(
                                Settings::new("castle-settings")
                                    .with_size(Size::Medium)
                                    .with_group_variant(GroupBoxVariant::Outline)
                                    .sidebar_width(px(sidebar_width))
                                    .header_style(&settings_header_style(sidebar_width))
                                    .pages(setting_pages(app.clone(), cx)),
                            ),
                        )
                    }
                })
        });
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

fn setting_pages(app: gpui::Entity<AppShell>, cx: &mut App) -> Vec<SettingPage> {
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
                                let app = app.clone();
                                move |cx: &App| !app.read(cx).sidebar.read(cx).is_collapsed()
                            },
                            {
                                let app = app.clone();
                                move |visible: bool, cx: &mut App| {
                                    app.update(cx, |app, cx| {
                                        app.set_sidebar_visible(visible, cx);
                                    });
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
                            |cx: &App| scrollbar_show_key(cx.theme().scrollbar_show),
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
                        SettingField::input(
                            AppSettings::tray_shortcut,
                            AppSettings::set_tray_shortcut,
                        )
                        .default_value(crate::app_settings::DEFAULT_TRAY_SHORTCUT),
                    )
                    .description(
                        "Global shortcut used to restore Castle, for example Ctrl+Alt+Space.",
                    ),
                ]),
            ]),
        SettingPage::new("Editor")
            .icon(Icon::new(IconName::BookOpen))
            .group(SettingGroup::new().title("Markdown").items(vec![
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
                        .description("Choose the monospace font family used while writing notes.")
                        .layout(Axis::Vertical),
                        SettingItem::new(
                            "Source Font Size",
                            SettingField::number_input(
                                NumberFieldOptions {
                                    min: 10.0,
                                    max: 22.0,
                                    step: 1.0,
                                },
                                AppSettings::markdown_font_size,
                                AppSettings::set_markdown_font_size,
                            )
                            .default_value(13.0),
                        )
                        .description("Adjust the monospace font size used while writing notes."),
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
                        .description("Adjust the font size used while reading rendered notes."),
                        SettingItem::new(
                            "Default Note View",
                            SettingField::dropdown(
                                vec![
                                    (EditorMode::Source.as_str().into(), "Write".into()),
                                    (EditorMode::Preview.as_str().into(), "Read".into()),
                                ],
                                AppSettings::markdown_editor_mode,
                                AppSettings::set_markdown_editor_mode,
                            )
                            .default_value(EditorMode::Source.as_str()),
                        )
                        .description("Choose the view used when a note editor opens."),
                        SettingItem::new(
                            "Status Line",
                            SettingField::switch(
                                AppSettings::markdown_status_line_visible,
                                AppSettings::set_markdown_status_line_visible,
                            )
                            .default_value(true),
                        )
                        .description(
                            "Show file, save state, view controls, and document statistics below note editors.",
                        ),
                        SettingItem::new(
                            "Line Numbers",
                            SettingField::switch(
                                AppSettings::markdown_line_numbers,
                                AppSettings::set_markdown_line_numbers,
                            )
                            .default_value(false),
                        )
                        .description("Show line numbers in newly opened note editors."),
                        SettingItem::new(
                            "Soft Wrap",
                            SettingField::switch(
                                AppSettings::markdown_soft_wrap,
                                AppSettings::set_markdown_soft_wrap,
                            )
                            .default_value(true),
                        )
                        .description("Wrap long lines in newly opened note editors."),
                    ])),
        SettingPage::new("Shortcuts")
            .icon(Icon::new(IconName::SquareTerminal))
            .description("Keyboard shortcuts currently registered by Castle.")
            .resettable(false)
            .groups(shortcut_groups(cx)),
        SettingPage::new("About")
            .icon(Icon::new(IconName::Info))
            .group(
                SettingGroup::new()
                    .title("Castle")
                    .items(vec![SettingItem::render(|options, _, cx| {
                        gpui_component::h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                gpui_component::v_flex().gap_1().child("Castle").child(
                                    gpui::div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("A private notes and kanban workspace."),
                                ),
                            )
                            .child(
                                gpui_component::button::Button::new("settings-about-version")
                                    .label(env!("CARGO_PKG_VERSION"))
                                    .outline()
                                    .with_size(options.size)
                                    .tab_stop(false),
                            )
                            .into_any_element()
                    })]),
            ),
    ]
}

fn shortcut_groups(cx: &App) -> Vec<SettingGroup> {
    let mut contexts = std::collections::BTreeMap::<SharedString, Vec<_>>::new();

    for shortcut in shortcuts(cx).iter().cloned() {
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
                        gpui_component::h_flex()
                            .w_full()
                            .min_h(rems(2.25))
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child(shortcut.action.clone()),
                            )
                            .child(
                                gpui_component::h_flex().flex_shrink_0().gap_1().children(
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

fn shortcut_context_name(context: &str) -> SharedString {
    match context {
        "AppShell" => "Application".into(),
        "CommandPalette" => "Command Palette".into(),
        "MarkdownEditor" => "Markdown Editor".into(),
        "EmmetInput" => "Emmet Input".into(),
        "TextView" => "Text View".into(),
        _ => humanize_identifier(context),
    }
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
            render_options.page_ix, render_options.group_ix, render_options.item_ix
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
                .with_size(render_options.size)
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
