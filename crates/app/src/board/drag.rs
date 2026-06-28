use gpui::{size, *};
use gpui_component::{ActiveTheme, h_flex};

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
        let size = size(px(200.), px(40.));

        div()
            .pl(self.position.x - size.width.half())
            .pt(self.position.y - size.height.half())
            .child(
                div()
                    .flex()
                    .justify_start()
                    .items_center()
                    .w(size.width)
                    .min_h(size.height)
                    .p_2()
                    .bg(cx.theme().primary.opacity(0.7))
                    .text_color(cx.theme().primary_foreground)
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .shadow_md()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .child(self.title.clone()),
                    ),
            )
    }
}

impl CardDragInfo {
    pub(crate) fn new(card_id: u32, source_board_id: u32, title: SharedString) -> Self {
        Self {
            card_id,
            source_board_id,
            position: Point::default(),
            title,
        }
    }

    pub(crate) fn position(mut self, pos: Point<Pixels>) -> Self {
        self.position = pos;
        self
    }
}

impl Render for CardDragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let size = size(px(320.), px(56.));

        div()
            .pl(self.position.x - size.width.half())
            .pt(self.position.y - size.height.half())
            .child(
                h_flex()
                    .w(size.width)
                    .h(size.height)
                    .gap_2()
                    .items_center()
                    .p_3()
                    .bg(cx.theme().secondary.opacity(0.92))
                    .text_color(cx.theme().secondary_foreground)
                    .border_1()
                    .border_color(cx.theme().primary)
                    .rounded(cx.theme().radius)
                    .shadow_lg()
                    .child("⋮⋮")
                    .child(self.title.clone()),
            )
    }
}
