use gpui::{
    App, Context, IntoElement, ParentElement, SharedString, StyleRefinement, Styled, Window, div,
    px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, Size, ThemeRegistry, WindowExt as _,
    group_box::GroupBoxVariant,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
};

use crate::app_settings::{AppSettings, scrollbar_show_key};
use crate::markdown_editor::types::EditorMode;

use super::AppShell;

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

        window.open_dialog(cx, move |dialog, _, _cx| {
            dialog
                .w(px(960.))
                .h(px(640.))
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
                        content.px_3().pb_3().child(
                            div().size_full().overflow_hidden().child(
                                Settings::new("castle-settings")
                                    .with_size(Size::Medium)
                                    .with_group_variant(GroupBoxVariant::Outline)
                                    .sidebar_width(px(260.))
                                    .header_style(&settings_header_style())
                                    .pages(setting_pages(app.clone(), cx)),
                            ),
                        )
                    }
                })
        });
    }
}

fn settings_header_style() -> StyleRefinement {
    StyleRefinement::default().w_full().flex_1()
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
                        SettingField::scrollable_dropdown(
                            theme_options(cx),
                            |cx: &App| cx.theme().theme_name().clone(),
                            |theme_name: SharedString, cx: &mut App| {
                                AppSettings::set_theme_name(theme_name, cx);
                            },
                        ),
                    )
                    .description("Choose the color theme used across Castle."),
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
            ]),
        SettingPage::new("Editor")
            .icon(Icon::new(IconName::BookOpen))
            .group(SettingGroup::new().title("Markdown").items(vec![
                        SettingItem::new(
                            "Editor Font Size",
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
                            "Default Note View",
                            SettingField::dropdown(
                                vec![
                                    (EditorMode::Source.as_str().into(), "Write".into()),
                                    (EditorMode::Split.as_str().into(), "Split".into()),
                                    (EditorMode::Preview.as_str().into(), "Read".into()),
                                ],
                                AppSettings::markdown_editor_mode,
                                AppSettings::set_markdown_editor_mode,
                            )
                            .default_value(EditorMode::Source.as_str()),
                        )
                        .description("Choose the view used when a note editor opens."),
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

fn theme_options(cx: &App) -> Vec<(SharedString, SharedString)> {
    ThemeRegistry::global(cx)
        .sorted_themes()
        .iter()
        .map(|theme| (theme.name.clone(), theme.name.clone()))
        .collect()
}
