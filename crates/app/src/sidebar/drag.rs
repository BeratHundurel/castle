use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, h_flex, v_flex};

use super::content_item::SidebarContentItem;

const PREVIEW_WIDTH: f32 = 196.;
const PREVIEW_OFFSET_X: f32 = 12.;
const PREVIEW_OFFSET_Y: f32 = 10.;

#[derive(Clone)]
pub(super) enum SidebarDragKind {
    Project { id: u32, source_index: usize },
    Content(SidebarContentItem),
}

#[derive(Clone)]
pub(super) struct SidebarDragInfo {
    pub(super) kind: SidebarDragKind,
    position: Point<Pixels>,
    title: SharedString,
    label: SharedString,
    detail: SharedString,
    icon: IconName,
}

impl SidebarDragInfo {
    pub(super) fn project(
        id: u32,
        source_index: usize,
        title: SharedString,
        item_count: usize,
    ) -> Self {
        Self {
            kind: SidebarDragKind::Project { id, source_index },
            position: Point::default(),
            title,
            label: "Project".into(),
            detail: format!(
                "{} {}",
                item_count,
                if item_count == 1 { "item" } else { "items" }
            )
            .into(),
            icon: IconName::FolderOpen,
        }
    }

    pub(super) fn content(item: SidebarContentItem, origin: SharedString) -> Self {
        Self {
            position: Point::default(),
            title: item.title(),
            label: item.kind_label().into(),
            detail: format!("From {origin}").into(),
            icon: item.icon(),
            kind: SidebarDragKind::Content(item),
        }
    }

    pub(super) fn position(mut self, position: Point<Pixels>) -> Self {
        self.position = position;
        self
    }

    pub(super) fn can_drop_on_project(&self, project_id: u32) -> bool {
        match &self.kind {
            SidebarDragKind::Project { id, .. } => *id != project_id,
            SidebarDragKind::Content(item) => item.can_move_to(Some(project_id)),
        }
    }

    pub(super) fn can_drop_on_standalone(&self) -> bool {
        matches!(&self.kind, SidebarDragKind::Content(item) if item.can_move_to(None))
    }
}

impl Render for SidebarDragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .pl(self.position.x + px(PREVIEW_OFFSET_X))
            .pt(self.position.y + px(PREVIEW_OFFSET_Y))
            .child(
                h_flex()
                    .w(px(PREVIEW_WIDTH))
                    .relative()
                    .overflow_hidden()
                    .gap_2()
                    .p_2()
                    .pl_3()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.drag_border)
                    .bg(theme.popover.opacity(0.97))
                    .text_color(theme.popover_foreground)
                    .shadow_md()
                    .opacity(0.98)
                    .child(
                        div()
                            .absolute()
                            .left(px(1.))
                            .top(px(1.))
                            .bottom(px(1.))
                            .w(px(2.))
                            .rounded_l(px(7.))
                            .bg(theme.primary),
                    )
                    .child(
                        div()
                            .flex()
                            .size_6()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .bg(theme.primary.opacity(0.12))
                            .child(
                                Icon::new(self.icon.clone())
                                    .xsmall()
                                    .text_color(theme.primary),
                            ),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap(px(1.))
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap_1()
                                    .text_xs()
                                    .line_height(relative(1.))
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(self.label.clone()),
                                    )
                                    .child("·")
                                    .child(div().min_w_0().truncate().child(self.detail.clone())),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .line_height(relative(1.1))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.title.clone()),
                            ),
                    ),
            )
    }
}
