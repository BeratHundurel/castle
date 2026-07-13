use anyhow::Result;
use entity::{board, note, project};
use gpui::{Context, SharedString, Window};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};

use crate::DB;

use super::{SidebarView, action::*, dto::*, event::SidebarEvent};

impl SidebarView {
    pub(super) fn restore_trashed(
        &mut self,
        kind: crate::trash::TrashItemKind,
        id: u32,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| -> Result<()> {
            runtime
                .spawn(async move {
                    crate::trash::restore_item(
                        db.as_ref(),
                        crate::trash::RestoreTrashItem(crate::trash::MoveToTrash { kind, id }),
                    )
                    .await
                })
                .await??;
            this.update(cx, |this, cx| {
                this.list_projects(cx);
                cx.emit(SidebarEvent::WorkspaceChanged);
            })
            .ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn set_board_pinned(&mut self, board_id: u32, pinned: bool, cx: &mut Context<Self>) {
        for board in self
            .projects
            .iter_mut()
            .flat_map(|project| project.boards.iter_mut())
            .chain(self.standalone_boards.iter_mut())
        {
            if board.id == board_id {
                board.is_pinned = pinned;
                break;
            }
        }
        cx.notify();
        cx.emit(SidebarEvent::WorkspaceChanged);
        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |this, cx| -> Result<()> {
            crate::home::set_pinned(
                db.as_ref(),
                crate::home::WorkspaceItemKind::Board,
                board_id,
                pinned,
            )
            .await?;
            this.update(cx, |this, cx| this.list_projects(cx)).ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn set_note_pinned(&mut self, note_id: u32, pinned: bool, cx: &mut Context<Self>) {
        for note in self
            .projects
            .iter_mut()
            .flat_map(|project| project.notes.iter_mut())
            .chain(self.standalone_notes.iter_mut())
        {
            if note.id == note_id {
                note.is_pinned = pinned;
                break;
            }
        }
        cx.notify();
        cx.emit(SidebarEvent::WorkspaceChanged);
        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |this, cx| -> Result<()> {
            crate::home::set_pinned(
                db.as_ref(),
                crate::home::WorkspaceItemKind::Note,
                note_id,
                pinned,
            )
            .await?;
            this.update(cx, |this, cx| this.list_projects(cx)).ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn select_board(
        &mut self,
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.active_project_id = project_id;
        self.active_item = Some(ActiveItem::Board(board_id));
        cx.emit(SidebarEvent::OpenBoard {
            board_id,
            project_id,
            title,
        });
    }

    pub(super) fn select_note(
        &mut self,
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.active_project_id = project_id;
        self.active_item = Some(ActiveItem::Note(note_id));
        cx.emit(SidebarEvent::OpenNote {
            note_id,
            project_id,
            title,
        });
    }

    pub(super) fn delete_board(&mut self, cx: &mut Context<Self>, board_id: u32) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| -> Result<()> {
            runtime
                .spawn(async move {
                    crate::trash::move_to_trash(
                        db.as_ref(),
                        crate::trash::MoveToTrash {
                            kind: crate::trash::TrashItemKind::Board,
                            id: board_id,
                        },
                        crate::markdown_editor::now_ts(),
                    )
                    .await
                })
                .await??;
            this.update(cx, |this, cx| {
                this.standalone_boards.retain(|board| board.id != board_id);
                for project in &mut this.projects {
                    project.boards.retain(|board| board.id != board_id);
                }
                this.renaming_board = None;
                cx.emit(SidebarEvent::BoardDeleted { board_id });
                cx.emit(SidebarEvent::WorkspaceChanged);
                cx.notify();
            })
            .ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn rename_board(&mut self, cx: &mut Context<Self>, board_id: u32, title: String) {
        for board in self
            .projects
            .iter_mut()
            .flat_map(|project| project.boards.iter_mut())
            .chain(self.standalone_boards.iter_mut())
        {
            if board.id == board_id {
                board.title = SharedString::from(title.as_str());
                break;
            }
        }

        cx.notify();
        cx.emit(SidebarEvent::BoardRenamed {
            board_id,
            title: SharedString::from(title.as_str()),
        });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            board::ActiveModel {
                id: Set(board_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok(())
        })
        .detach();
    }

    pub(super) fn delete_note(
        &mut self,
        note_id: u32,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn_in(window, async move |this, cx| {
            let request = crate::trash::MoveToTrash {
                kind: crate::trash::TrashItemKind::Note,
                id: note_id,
            };
            let result = match runtime
                .spawn(async move {
                    crate::trash::move_to_trash(
                        db.as_ref(),
                        request,
                        crate::markdown_editor::now_ts(),
                    )
                    .await
                })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    this.standalone_notes.retain(|note| note.id != note_id);
                    for project in &mut this.projects {
                        project.notes.retain(|note| note.id != note_id);
                    }
                    this.renaming_note = None;
                    cx.emit(SidebarEvent::NoteDeleted { note_id });
                    cx.emit(SidebarEvent::WorkspaceChanged);
                    this.push_trash_undo(
                        crate::trash::TrashItemKind::Note,
                        note_id,
                        title.clone(),
                        window,
                        cx,
                    );
                    cx.notify();
                }
                Err(err) => {
                    eprintln!("Failed to move note to Trash: {err}");
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn rename_note(&mut self, cx: &mut Context<Self>, note_id: u32, title: String) {
        let shared_title = SharedString::from(title.as_str());

        if let Some(note) = self
            .projects
            .iter_mut()
            .flat_map(|project| project.notes.iter_mut())
            .chain(self.standalone_notes.iter_mut())
            .find(|note| note.id == note_id)
        {
            note.title = shared_title.clone();
        }
        cx.notify();

        cx.emit(SidebarEvent::NoteRenamed {
            note_id,
            title: shared_title,
        });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            note::ActiveModel {
                id: Set(note_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok(())
        })
        .detach();
    }

    pub(super) fn start_renaming_board(
        &mut self,
        action: &EditBoardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self
            .find_board(action.0)
            .map(|board| board.title.to_string())
        else {
            return;
        };

        self.renaming_board = Some(action.0);
        self.rename_board_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn start_renaming_note(
        &mut self,
        action: &EditNoteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self.find_note(action.0).map(|note| note.title.to_string()) else {
            return;
        };

        self.renaming_note = Some(action.0);
        self.rename_note_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn move_board(
        &mut self,
        cx: &mut Context<Self>,
        board_id: u32,
        project_id: Option<u32>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |this, cx| -> Result<()> {
            board::ActiveModel {
                id: Set(board_id as i64),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .update(&*db)
            .await?;

            this.update(cx, |this, cx| this.list_projects(cx)).ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn move_note(
        &mut self,
        cx: &mut Context<Self>,
        note_id: u32,
        project_id: Option<u32>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |this, cx| -> Result<()> {
            note::ActiveModel {
                id: Set(note_id as i64),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .update(&*db)
            .await?;

            this.update(cx, |this, cx| this.list_projects(cx)).ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn start_renaming_project(
        &mut self,
        action: &RenameProjectAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .find_project(action.0)
            .map(|project| project.name.to_string())
        else {
            return;
        };

        self.renaming_project = Some(action.0);
        self.rename_project_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn rename_project(&mut self, cx: &mut Context<Self>, project_id: u32, name: String) {
        let shared_name = SharedString::from(name.as_str());

        if let Some(project) = self
            .projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            project.name = shared_name.clone();
        }

        cx.notify();
        cx.emit(SidebarEvent::ProjectRenamed {
            project_id,
            name: shared_name,
        });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            project::ActiveModel {
                id: Set(project_id as i64),
                name: Set(name),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok(())
        })
        .detach();
    }

    pub(super) fn delete_project(&mut self, cx: &mut Context<Self>, project_id: u32) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| -> Result<()> {
            runtime
                .spawn(async move {
                    crate::trash::move_to_trash(
                        db.as_ref(),
                        crate::trash::MoveToTrash {
                            kind: crate::trash::TrashItemKind::Project,
                            id: project_id,
                        },
                        crate::markdown_editor::now_ts(),
                    )
                    .await
                })
                .await??;
            this.update(cx, |this, cx| {
                this.projects.retain(|project| project.id != project_id);
                this.renaming_project = None;
                if this.active_project_id == Some(project_id) {
                    this.active_project_id = None;
                    this.active_item = None;
                }
                this.list_projects(cx);
                cx.emit(SidebarEvent::ProjectDeleted { project_id });
                cx.emit(SidebarEvent::WorkspaceChanged);
                cx.notify();
            })
            .ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn move_project_up(&mut self, cx: &mut Context<Self>, project_id: u32) {
        let Some(index) = self
            .projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return;
        };

        if index == 0 {
            return;
        }

        self.projects.swap(index - 1, index);
        self.persist_project_positions(cx);
    }

    pub(super) fn move_project_down(&mut self, cx: &mut Context<Self>, project_id: u32) {
        let Some(index) = self
            .projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return;
        };

        if index + 1 >= self.projects.len() {
            return;
        }

        self.projects.swap(index, index + 1);
        self.persist_project_positions(cx);
    }

    fn persist_project_positions(&mut self, cx: &mut Context<Self>) {
        let positions: Vec<(u32, i32)> = self
            .projects
            .iter_mut()
            .enumerate()
            .map(|(index, project)| {
                project.position = index as i32;
                (project.id, project.position)
            })
            .collect();

        cx.notify();
        cx.emit(SidebarEvent::ProjectsReordered);

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            for (project_id, position) in positions {
                project::ActiveModel {
                    id: Set(project_id as i64),
                    position: Set(position),
                    ..Default::default()
                }
                .update(&*db)
                .await?;
            }

            Ok(())
        })
        .detach();
    }
}
