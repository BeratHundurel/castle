use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, h_flex, v_flex};
use gpui_kit::*;

const PREVIEW_WIDTH: f32 = 196.;
const PREVIEW_OFFSET_X: f32 = 12.;
const PREVIEW_OFFSET_Y: f32 = 10.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceDragKind {
    Project { id: u32, source_index: usize },
    Board { id: u32, project_id: Option<u32> },
    Note { id: u32, project_id: Option<u32> },
}

impl WorkspaceDragKind {
    pub fn note_id(self) -> Option<i64> {
        match self {
            Self::Note { id, .. } => Some(i64::from(id)),
            Self::Project { .. } | Self::Board { .. } => None,
        }
    }

    pub fn is_content(self) -> bool {
        matches!(self, Self::Board { .. } | Self::Note { .. })
    }

    fn source_project_id(self) -> Option<Option<u32>> {
        match self {
            Self::Project { .. } => None,
            Self::Board { project_id, .. } | Self::Note { project_id, .. } => Some(project_id),
        }
    }
}

#[derive(Clone)]
pub struct WorkspaceDragInfo {
    kind: WorkspaceDragKind,
    position: Point<Pixels>,
    title: SharedString,
    label: SharedString,
    detail: SharedString,
    icon: IconName,
}

impl WorkspaceDragInfo {
    pub fn new(
        kind: WorkspaceDragKind,
        title: impl Into<SharedString>,
        label: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        icon: IconName,
    ) -> Self {
        Self {
            kind,
            position: Point::default(),
            title: title.into(),
            label: label.into(),
            detail: detail.into(),
            icon,
        }
    }

    pub fn kind(&self) -> WorkspaceDragKind {
        self.kind
    }

    pub fn note_id(&self) -> Option<i64> {
        self.kind.note_id()
    }

    pub fn position(mut self, position: Point<Pixels>) -> Self {
        self.position = position;
        self
    }

    pub fn can_drop_on_project(&self, project_id: u32) -> bool {
        match self.kind {
            WorkspaceDragKind::Project { id, .. } => id != project_id,
            WorkspaceDragKind::Board {
                project_id: source, ..
            }
            | WorkspaceDragKind::Note {
                project_id: source, ..
            } => source != Some(project_id),
        }
    }

    pub fn can_drop_on_standalone(&self) -> bool {
        self.kind
            .source_project_id()
            .is_some_and(|project_id| project_id.is_some())
    }
}

impl Render for WorkspaceDragInfo {
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

#[cfg(test)]
mod tests {
    use super::{WorkspaceDragInfo, WorkspaceDragKind};
    use gpui_kit::component::IconName;

    fn drag(kind: WorkspaceDragKind) -> WorkspaceDragInfo {
        WorkspaceDragInfo::new(kind, "Item", "Type", "From workspace", IconName::File)
    }

    #[test]
    fn note_identity_is_available_without_sidebar_types() {
        let note = drag(WorkspaceDragKind::Note {
            id: 42,
            project_id: Some(7),
        });
        let board = drag(WorkspaceDragKind::Board {
            id: 12,
            project_id: Some(7),
        });
        let project = drag(WorkspaceDragKind::Project {
            id: 7,
            source_index: 0,
        });

        assert_eq!(note.note_id(), Some(42));
        assert_eq!(
            note.kind(),
            WorkspaceDragKind::Note {
                id: 42,
                project_id: Some(7)
            }
        );
        assert_eq!(board.note_id(), None);
        assert_eq!(project.note_id(), None);
    }

    #[test]
    fn drop_targets_reject_the_current_location() {
        let standalone_note = drag(WorkspaceDragKind::Note {
            id: 1,
            project_id: None,
        });
        let project_board = drag(WorkspaceDragKind::Board {
            id: 2,
            project_id: Some(10),
        });
        let project = drag(WorkspaceDragKind::Project {
            id: 10,
            source_index: 1,
        });

        assert!(!standalone_note.can_drop_on_standalone());
        assert!(standalone_note.can_drop_on_project(10));
        assert!(project_board.can_drop_on_standalone());
        assert!(!project_board.can_drop_on_project(10));
        assert!(project_board.can_drop_on_project(11));
        assert!(!project.can_drop_on_project(10));
        assert!(project.can_drop_on_project(11));
    }
}
