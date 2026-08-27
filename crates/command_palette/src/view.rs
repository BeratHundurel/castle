use gpui::{
    ClickEvent, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px,
    relative,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    v_flex,
};

use crate::{CommandPaletteMode, CommandPaletteView, PaletteCommand};

impl CommandPaletteView {
    pub(crate) fn render_command_palette_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.mode == CommandPaletteMode::Search {
            return self.render_workspace_search_overlay(cx);
        }

        let theme = cx.theme().clone();
        let title = match self.mode {
            CommandPaletteMode::Commands => "Command Palette",
            CommandPaletteMode::Themes => "Switch Theme",
            CommandPaletteMode::Search => "Search Workspace",
        };

        div()
            .id("command-palette-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_start()
            .justify_center()
            .pt_12()
            .bg(theme.transparent)
            .key_context("CommandPalette")
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close(window, cx);
                }),
            )
            .child(
                v_flex()
                    .id("command-palette-panel")
                    .w(px(620.))
                    .max_h(px(520.))
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.popover)
                    .shadow_md()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.popover_foreground)
                                    .child(title),
                            )
                            .when(self.mode != CommandPaletteMode::Commands, |this| {
                                this.child(
                                    Button::new("command-palette-back")
                                        .label("Back")
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.mode = CommandPaletteMode::Commands;
                                            this.query.clear();
                                            this.search_generation =
                                                this.search_generation.saturating_add(1);
                                            this.search_debounce_task = None;
                                            this.search_results.clear();
                                            this.search_loading = false;
                                            this.search_error = None;
                                            this.selected_index = 0;
                                            this.scroll_handle.scroll_to_item(0);
                                            this.suppress_input_event = true;
                                            this.input.update(cx, |input, cx| {
                                                input.set_placeholder("Type a command", window, cx);
                                                input.set_value("", window, cx);
                                                input.focus(window, cx);
                                            });
                                            this.suppress_input_event = false;
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(
                                Button::new("command-palette-close")
                                    .icon(IconName::Close)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Close")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        Input::new(&self.input)
                            .border_0()
                            .rounded_none()
                            .bg(theme.popover)
                            .prefix(IconName::Search),
                    )
                    .child(match self.mode {
                        CommandPaletteMode::Commands => self.render_command_results(cx),
                        CommandPaletteMode::Themes => self.render_theme_results(cx),
                        CommandPaletteMode::Search => self.render_search_results(cx),
                    }),
            )
            .into_any_element()
    }

    fn render_command_results(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let commands = self.command_palette_commands();
        let theme = cx.theme().clone();

        v_flex()
            .id("command-palette-results")
            .relative()
            .gap_1()
            .p_2()
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .when_else(
                commands.is_empty(),
                |this| {
                    this.child(
                        div()
                            .px_3()
                            .py_6()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No commands found"),
                    )
                },
                |this| {
                    this.children(commands.into_iter().enumerate().map(|(index, command)| {
                        self.render_command_row(index, command, index == self.selected_index, cx)
                    }))
                },
            )
            .into_any_element()
    }

    fn render_theme_results(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let themes = self.filtered_theme_names(cx);
        let current_theme = cx.theme().theme_name().clone();
        let theme = cx.theme().clone();

        v_flex()
            .id("theme-palette-results")
            .relative()
            .gap_1()
            .p_2()
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .when_else(
                themes.is_empty(),
                |this| {
                    this.child(
                        div()
                            .px_3()
                            .py_6()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No themes found"),
                    )
                },
                |this| {
                    this.children(themes.into_iter().enumerate().map(|(index, theme_name)| {
                        let is_current = theme_name == current_theme;
                        self.render_theme_row(
                            index,
                            theme_name,
                            is_current,
                            index == self.selected_index,
                            cx,
                        )
                    }))
                },
            )
            .into_any_element()
    }

    fn render_command_row(
        &self,
        index: usize,
        command: PaletteCommand,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let label = command.label.clone();
        let subtitle = command.subtitle.clone();
        let icon = command.icon.clone();

        h_flex()
            .id(("command-row", index))
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(theme.radius)
            .text_color(theme.popover_foreground)
            .bg(if is_selected {
                theme.accent
            } else {
                theme.popover.opacity(0.)
            })
            .hover(|this| this.bg(theme.accent))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.execute_palette_command(command.clone(), window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_7()
                    .rounded(theme.radius)
                    .bg(theme.primary.opacity(0.14))
                    .text_color(theme.primary)
                    .child(Icon::new(icon).xsmall()),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_0()
                    .child(
                        div()
                            .text_sm()
                            .line_height(relative(1.25))
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .line_height(relative(1.25))
                            .text_color(theme.muted_foreground)
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(subtitle),
                    ),
            )
            .into_any_element()
    }

    fn render_theme_row(
        &self,
        index: usize,
        theme_name: SharedString,
        is_current: bool,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let label = theme_name.clone();

        h_flex()
            .id(("theme-row", index))
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(theme.radius)
            .text_color(theme.popover_foreground)
            .bg(if is_selected {
                theme.accent
            } else {
                theme.popover.opacity(0.)
            })
            .hover(|this| this.bg(theme.accent))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.apply_theme(&theme_name, cx);
                if event.click_count() == 2 {
                    this.close(window, cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_7()
                    .rounded(theme.radius)
                    .bg(theme.primary.opacity(0.14))
                    .text_color(theme.primary)
                    .child(Icon::new(IconName::Palette).xsmall()),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_sm()
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(label),
            )
            .when(is_current, |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .xsmall()
                        .text_color(theme.primary),
                )
            })
            .into_any_element()
    }
}

impl Render for CommandPaletteView {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.open {
            self.render_command_palette_overlay(cx)
        } else {
            div().into_any_element()
        }
    }
}
