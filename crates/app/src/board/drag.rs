use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, h_flex, v_flex};

const ENTRY_PREVIEW_WIDTH: f32 = 304.;
const CARD_PREVIEW_WIDTH: f32 = 320.;
const PREVIEW_OFFSET_X: f32 = 18.;
const PREVIEW_OFFSET_Y: f32 = 16.;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DragInfo {
    pub(crate) entry_id: u32,
    pub(crate) source_board_id: u32,
    pub(crate) source_card_id: u32,
    pub(crate) position: Point<Pixels>,
    pub(crate) title: SharedString,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CardDragInfo {
    pub(crate) card_id: u32,
    pub(crate) source_board_id: u32,
    pub(crate) position: Point<Pixels>,
    pub(crate) title: SharedString,
    pub(crate) entry_count: usize,
}

impl DragInfo {
    pub(crate) fn new(
        entry_id: u32,
        source_board_id: u32,
        source_card_id: u32,
        title: SharedString,
    ) -> Self {
        Self {
            entry_id,
            source_board_id,
            source_card_id,
            position: Point::default(),
            title,
        }
    }

    pub(crate) fn position(mut self, pos: Point<Pixels>) -> Self {
        self.position = pos;
        self
    }
}

impl Render for DragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .pl(self.position.x + px(PREVIEW_OFFSET_X))
            .pt(self.position.y + px(PREVIEW_OFFSET_Y))
            .child(
                div()
                    .w(px(ENTRY_PREVIEW_WIDTH))
                    .relative()
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.drag_border)
                    .bg(theme.popover.opacity(0.96))
                    .text_color(theme.popover_foreground)
                    .text_sm()
                    .shadow_lg()
                    .opacity(0.96)
                    .child(
                        div()
                            .absolute()
                            .left(px(1.))
                            .top(px(1.))
                            .bottom(px(1.))
                            .w(px(4.))
                            .bg(theme.primary)
                            .flex_shrink_0(),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .p_3()
                            .pl_4()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        Icon::new(IconName::BookOpen)
                                            .xsmall()
                                            .text_color(theme.muted_foreground),
                                    )
                                    .child("Card"),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .whitespace_normal()
                                    .line_height(relative(1.35))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(self.title.clone()),
                            ),
                    ),
            )
    }
}

impl CardDragInfo {
    pub(crate) fn new(
        card_id: u32,
        source_board_id: u32,
        title: SharedString,
        entry_count: usize,
    ) -> Self {
        Self {
            card_id,
            source_board_id,
            position: Point::default(),
            title,
            entry_count,
        }
    }

    pub(crate) fn position(mut self, pos: Point<Pixels>) -> Self {
        self.position = pos;
        self
    }
}

impl Render for CardDragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let entry_count = SharedString::from(format!(
            "{} {}",
            self.entry_count,
            if self.entry_count == 1 {
                "card"
            } else {
                "cards"
            }
        ));

        div()
            .pl(self.position.x + px(PREVIEW_OFFSET_X))
            .pt(self.position.y + px(PREVIEW_OFFSET_Y))
            .child(
                v_flex()
                    .w(px(CARD_PREVIEW_WIDTH))
                    .gap_3()
                    .p_3()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.drag_border)
                    .bg(theme.secondary.opacity(0.96))
                    .text_color(theme.secondary_foreground)
                    .shadow_lg()
                    .opacity(0.96)
                    .child(
                        h_flex()
                            .justify_between()
                            .gap_3()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap_2()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        Icon::new(IconName::LayoutDashboard)
                                            .xsmall()
                                            .text_color(theme.muted_foreground),
                                    )
                                    .child("List"),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .rounded(px(99.))
                                    .px_2()
                                    .py(px(2.))
                                    .bg(theme.primary.opacity(0.14))
                                    .text_color(theme.primary)
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(entry_count),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .line_height(relative(1.25))
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.title.clone()),
                    )
                    .child(h_flex().gap_1().children((0..3).map(|_| {
                        div()
                            .h(px(3.))
                            .flex_1()
                            .rounded(px(99.))
                            .bg(theme.drag_border.opacity(0.55))
                    }))),
            )
    }
}
