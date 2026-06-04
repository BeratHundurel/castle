use anyhow::Result;
use entity::{
    board, board::Entity as Board, note, note::Entity as Note, project, project::Entity as Project,
};
use gpui::{Context, SharedString, Window};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use std::{fs::remove_file, io::ErrorKind};

use crate::DB;

use super::{SidebarView, action::*, dto::*, event::SidebarEvent};

impl SidebarView {
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
        self.standalone_boards.retain(|board| board.id != board_id);
        for project in &mut self.projects {
            project.boards.retain(|board| board.id != board_id);
        }
        self.renaming_board = None;

        cx.notify();
        cx.emit(SidebarEvent::BoardDeleted { board_id });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            Board::delete_by_id(board_id as i64).exec(&*db).await?;
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

    pub(super) fn delete_note(&mut self, cx: &mut Context<Self>, note_id: u32) {
        self.standalone_notes.retain(|note| note.id != note_id);
        for project in &mut self.projects {
            project.notes.retain(|note| note.id != note_id);
        }
        self.renaming_note = None;
        cx.notify();
        cx.emit(SidebarEvent::NoteDeleted { note_id });

        let db = cx.global::<DB>().conn.clone();
        let background_executor = cx.background_executor().clone();
        cx.spawn(async move |_, _| -> Result<()> {
            if let Some(note) = Note::find_by_id(note_id as i64).one(&*db).await? {
                Note::delete_by_id(note_id as i64).exec(&*db).await?;

                if note.file_managed_by_app
                    && let Some(path) = note.file_path
                {
                    background_executor
                        .spawn(async move {
                            match remove_file(path) {
                                Ok(()) => Ok(()),
                                Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
                                Err(err) => Err(err),
                            }
                        })
                        .await?;
                }
            }

            Ok(())
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

            this.update(cx, |_, cx| Self::list_projects(cx)).ok();
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

            this.update(cx, |_, cx| Self::list_projects(cx)).ok();
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
        self.projects.retain(|project| project.id != project_id);
        self.renaming_project = None;
        if self.active_project_id == Some(project_id) {
            self.active_project_id = None;
            self.active_item = None;
        }

        cx.notify();
        cx.emit(SidebarEvent::ProjectDeleted { project_id });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |this, cx| -> Result<()> {
            Project::delete_by_id(project_id as i64).exec(&*db).await?;
            this.update(cx, |_, cx| Self::list_projects(cx)).ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn archive_project(&mut self, cx: &mut Context<Self>, project_id: u32) {
        self.projects.retain(|project| project.id != project_id);
        self.renaming_project = None;
        if self.active_project_id == Some(project_id) {
            self.active_project_id = None;
            self.active_item = None;
        }

        cx.notify();
        cx.emit(SidebarEvent::ProjectArchived { project_id });

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            project::ActiveModel {
                id: Set(project_id as i64),
                archived: Set(true),
                ..Default::default()
            }
            .update(&*db)
            .await?;
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
