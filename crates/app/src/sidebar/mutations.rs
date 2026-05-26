use anyhow::Result;
use entity::{board, board::Entity as Board, note, note::Entity as Note};
use gpui::{Context, SharedString, Window};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

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
        self.adding_board_to_project = None;
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
        cx.spawn(async move |_, _| -> Result<()> {
            Note::delete_by_id(note_id as i64).exec(&*db).await?;
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
}
