use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, relative,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, h_flex, input::Input,
    scroll::ScrollableElement as _, text::TextView, v_flex,
};

use crate::app_shell::AppShell;
use crate::command_palette::search_preview::{
    SearchPreviewBlock, highlighted_exact_search_text, render_highlighted_preview_line,
    search_preview_blocks, search_preview_markdown_style, search_result_preview_source,
    search_result_row_text,
};
use crate::search::SearchResult;

impl AppShell {
    pub(super) fn render_workspace_search_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let query_is_empty = self.command_palette.query.trim().is_empty();
        let result_count = if query_is_empty {
            0
        } else {
            self.command_palette.search_results.len()
        };

        div()
            .id("workspace-search-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_start()
            .justify_center()
            .pt_8()
            .bg(theme.overlay.opacity(0.78))
            .key_context("CommandPalette")
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_command_palette(window, cx);
                }),
            )
            .child(
                v_flex()
                    .id("workspace-search-panel")
                    .w(relative(0.94))
                    .max_w(px(996.))
                    .h(relative(0.72))
                    .max_h(px(720.))
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border.opacity(0.72))
                    .bg(theme.popover)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.primary)
                                    .child("find")
                                    .child(div().text_color(theme.muted_foreground).child(">")),
                            )
                            .child(
                                Input::new(&self.command_palette.input)
                                    .flex_1()
                                    .border_0()
                                    .rounded_none()
                                    .bg(theme.popover),
                            ),
                    )
                    .child(self.render_search_results(cx))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .px_4()
                            .py_2()
                            .border_t_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(search_footer_hint(IconName::ChevronsUpDown, "move"))
                                    .child(search_footer_hint(IconName::ArrowRight, "open"))
                                    .child(search_footer_hint(IconName::Close, "close")),
                            )
                            .child(format!("{result_count} results")),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_search_results(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let query_is_empty = self.command_palette.query.trim().is_empty();
        let result_count = self.command_palette.search_results.len();

        let container = v_flex()
            .id("search-palette-results")
            .relative()
            .flex_1()
            .min_h_0()
            .gap_1()
            .p_2()
            .pr_4()
            .overflow_y_scroll()
            .track_scroll(&self.command_palette.scroll_handle);

        if let Some(error) = self.command_palette.search_error.clone() {
            return container
                .child(
                    div()
                        .px_3()
                        .py_6()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error),
                )
                .into_any_element();
        }

        if query_is_empty {
            return h_flex()
                .items_stretch()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(
                    div()
                        .w(relative(0.38))
                        .h_full()
                        .min_w_0()
                        .min_h_0()
                        .px_8()
                        .py_8()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("type to search your workspace"),
                )
                .child(
                    div()
                        .w(relative(0.62))
                        .h_full()
                        .min_w_0()
                        .min_h_0()
                        .border_l_1()
                        .border_color(theme.border)
                        .px_8()
                        .py_8()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("no match"),
                )
                .into_any_element();
        }

        if self.command_palette.search_loading && result_count == 0 {
            return container.into_any_element();
        }

        if result_count == 0 {
            return container
                .child(
                    div()
                        .px_3()
                        .py_6()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("No results found"),
                )
                .into_any_element();
        }

        let results = container.children(
            self.command_palette
                .search_results
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, result)| {
                    self.render_search_result_row(
                        index,
                        result,
                        index == self.command_palette.selected_index,
                        cx,
                    )
                }),
        );

        let selected_result = self
            .command_palette
            .search_results
            .get(
                self.command_palette
                    .selected_index
                    .min(result_count.saturating_sub(1)),
            )
            .cloned();

        h_flex()
            .items_stretch()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                v_flex()
                    .w(relative(0.38))
                    .h_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(results)
                    .vertical_scrollbar(&self.command_palette.scroll_handle),
            )
            .child(
                div()
                    .w(relative(0.62))
                    .h_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .border_l_1()
                    .border_color(theme.border)
                    .child(self.render_search_preview(selected_result, cx)),
            )
            .into_any_element()
    }

    fn render_search_preview(
        &self,
        result: Option<SearchResult>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let Some(result) = result else {
            return div().size_full().into_any_element();
        };

        let preview_blocks = search_preview_blocks(
            search_result_preview_source(&result),
            &self.command_palette.query,
        );

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(result.title.clone()),
                    ),
            )
            .child(
                v_flex()
                    .id("search-result-preview-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_4()
                    .text_size(px(13.))
                    .line_height(relative(1.55))
                    .text_color(theme.popover_foreground)
                    .child(
                        v_flex()
                            .gap_2()
                            .children(preview_blocks.into_iter().enumerate().map(
                                |(block_index, block)| {
                                    self.render_search_preview_block(
                                        &result,
                                        block_index,
                                        block,
                                        cx,
                                    )
                                },
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_search_preview_block(
        &self,
        result: &SearchResult,
        block_index: usize,
        block: SearchPreviewBlock,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();

        if block.is_match {
            return v_flex()
                .id(("search-result-preview-match", block_index))
                .w_full()
                .min_w_0()
                .gap_1()
                .px_3()
                .py_2()
                .rounded(theme.radius)
                .bg(theme.accent.opacity(0.55))
                .border_1()
                .border_color(theme.primary.opacity(0.16))
                .children(
                    block
                        .markdown
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .map(|line| {
                            render_highlighted_preview_line(line, &self.command_palette.query, cx)
                        }),
                )
                .into_any_element();
        }

        TextView::markdown(
            format!(
                "search-result-preview-markdown-{}-{}",
                result.item_id, block_index
            ),
            gpui::SharedString::from(block.markdown),
        )
        .style(search_preview_markdown_style())
        .scrollable(false)
        .selectable(true)
        .into_any_element()
    }

    fn render_search_result_row(
        &self,
        index: usize,
        result: SearchResult,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let row_text = search_result_row_text(&result);
        let label = highlighted_exact_search_text(&row_text, &self.command_palette.query, cx);

        h_flex()
            .id(("search-result-row", index))
            .flex_none()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(theme.radius)
            .border_1()
            .text_color(theme.popover_foreground)
            .border_color(if is_selected {
                theme.primary.opacity(0.18)
            } else {
                theme.border.opacity(0.)
            })
            .bg(if is_selected {
                theme.accent.opacity(0.68)
            } else {
                theme.popover.opacity(0.)
            })
            .hover(|this| this.bg(theme.accent.opacity(0.5)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_search_result(result.clone(), window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_weight(if is_selected {
                        gpui::FontWeight::SEMIBOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .line_height(relative(1.35))
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(label),
            )
            .into_any_element()
    }
}

fn search_footer_hint(icon: IconName, label: &'static str) -> gpui::AnyElement {
    h_flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).xsmall())
        .child(label)
        .into_any_element()
}
