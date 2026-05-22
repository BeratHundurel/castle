use gpui::SharedString;

#[derive(Clone)]
pub(crate) enum SidebarEvent {
    OpenBoard {
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
    },
    OpenNote {
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
    },
    ActivateProject {
        project_id: u32,
    },
}