use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, h_flex,
    input::Input,
    select::Select,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem,
    },
    v_flex,
};

use super::SidebarView;
use super::action::*;
use super::dto::*;
use super::event::SidebarEvent;

impl EventEmitter<SidebarEvent> for SidebarView {}

impl Focusable for SidebarView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SidebarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let adding_board_to_project = self.adding_board_to_project;
        let search_query = self.search_input.read(cx).text().to_string();
        let search_lower = search_query.to_lowercase();

        let project_menu_items: Vec<SidebarMenuItem> = self
            .projects
            .iter()
            .filter_map(|project| {
                let project_matches =
                    search_lower.is_empty() || project.name.to_lowercase().contains(&search_lower);
                let filtered_boards = project
                    .boards
                    .iter()
                    .filter(|board| {
                        project_matches || board.title.to_lowercase().contains(&search_lower)
                    })
                    .collect::<Vec<_>>();
                let filtered_notes = project
                    .notes
                    .iter()
                    .filter(|note| {
                        project_matches || note.title.to_lowercase().contains(&search_lower)
                    })
                    .collect::<Vec<_>>();

                if !project_matches && filtered_boards.is_empty() && filtered_notes.is_empty() {
                    return None;
                }

                let project_id = project.id;
                let mut children: Vec<SidebarMenuItem> = Vec::new();
                children.extend(
                    filtered_notes
                        .into_iter()
                        .map(|note| self.render_note_item(note, cx, true)),
                );
                children.extend(
                    filtered_boards
                        .into_iter()
                        .map(|board| self.render_board_item(board, cx, true)),
                );

                if adding_board_to_project == Some(Some(project_id)) {
                    let input = self.new_board_input.clone();
                    children.push(SidebarMenuItem::new("").disable(true).suffix(
                        move |_window, cx| {
                            Input::new(&input)
                                .small()
                                .bg(cx.theme().sidebar)
                                .rounded_none()
                                .focus_bordered(false)
                                .border_0()
                                .border_b_1()
                                .border_color(cx.theme().foreground)
                                .text_xs()
                                .w_full()
                        },
                    ));
                } else {
                    children.push(
                        SidebarMenuItem::new("Add board")
                            .icon(IconName::Plus)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.adding_board_to_project = Some(Some(project_id));
                                this.new_board_input.update(cx, |input, cx| {
                                    input.focus(window, cx);
                                });
                                cx.notify();
                            })),
                    );
                }

                Some(
                    SidebarMenuItem::new(project.name.clone())
                        .icon(IconName::FolderOpen)
                        .active(self.active_project_id == Some(project_id))
                        .default_open(project.is_expanded || !search_lower.is_empty())
                        .click_to_toggle(true)
                        .children(children)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.active_project_id = Some(project_id);
                            cx.emit(SidebarEvent::ActivateProject { project_id });
                            this.focus_handle.focus(window, cx);
                            cx.notify();
                        })),
                )
            })
            .collect();

        let standalone_matches = search_lower.is_empty() || "standalone".contains(&search_lower);
        let standalone_boards = self
            .standalone_boards
            .iter()
            .filter(|board| {
                standalone_matches || board.title.to_lowercase().contains(&search_lower)
            })
            .collect::<Vec<_>>();

        let standalone_notes = self
            .standalone_notes
            .iter()
            .filter(|note| standalone_matches || note.title.to_lowercase().contains(&search_lower))
            .collect::<Vec<_>>();

        let mut standalone_items: Vec<SidebarMenuItem> = standalone_notes
            .into_iter()
            .map(|note| self.render_note_item(note, cx, true))
            .collect();

        standalone_items.extend(
            standalone_boards
                .into_iter()
                .map(|board| self.render_board_item(board, cx, true)),
        );

        div()
            .h_full()
            .flex_shrink_0()
            .on_action(cx.listener(Self::on_delete_board_action))
            .on_action(cx.listener(Self::on_edit_board_action))
            .on_action(cx.listener(Self::on_move_board_action))
            .on_action(cx.listener(Self::on_move_note_action))
            .on_action(cx.listener(Self::on_delete_note_action))
            .on_action(cx.listener(Self::on_edit_note_action))
            .child(
                Sidebar::new("sidebar")
                    .w(px(260.))
                    .collapsible(SidebarCollapsible::Offcanvas)
                    .collapsed(self.collapsed)
                    .border_0()
                    .gap_0()
                    .header(
                        v_flex()
                            .id("header")
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                SidebarHeader::new()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(theme.radius)
                                            .bg(theme.primary)
                                            .text_color(theme.primary_foreground)
                                            .size_8()
                                            .flex_shrink_0()
                                            .child(IconName::GalleryVerticalEnd),
                                    )
                                    .child(
                                        v_flex()
                                            .id("header-title")
                                            .gap_0()
                                            .text_sm()
                                            .flex_1()
                                            .line_height(relative(1.25))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child("Castle")
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.sidebar_foreground)
                                            .child(
                                                div()
                                                    .child("Your private note taking app")
                                                    .text_color(theme.muted_foreground)
                                                    .text_xs(),
                                            ),
                                    ),
                            )
                            .child(
                                Input::new(&self.search_input)
                                    .cleanable(true)
                                    .prefix(IconName::Search),
                            )
                            .child({
                                if self.is_adding_project {
                                    Input::new(&self.new_project_input)
                                        .w_full()
                                        .rounded_none()
                                        .focus_bordered(false)
                                        .border_0()
                                        .border_b_1()
                                        .border_color(theme.foreground)
                                        .into_any_element()
                                } else {
                                    div()
                                        .id("add-project-btn-container")
                                        .flex()
                                        .w_full()
                                        .justify_center()
                                        .items_center()
                                        .h_8()
                                        .rounded(theme.radius)
                                        .bg(theme.accent_foreground.opacity(0.15))
                                        .hover(|this| {
                                            this.bg(theme.accent_foreground.opacity(0.20))
                                        })
                                        .border_1()
                                        .border_color(theme.accent_foreground.opacity(0.30))
                                        .cursor_pointer()
                                        .child(
                                            h_flex()
                                                .id("add-project-btn")
                                                .w_full()
                                                .justify_center()
                                                .items_center()
                                                .gap_1()
                                                .text_sm()
                                                .text_color(theme.accent_foreground)
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(IconName::Plus)
                                                .child("Add Project"),
                                        )
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.is_adding_project = true;
                                            this.new_project_input.update(cx, |input, cx| {
                                                input.focus(window, cx);
                                            });
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                }
                            }),
                    )
                    .child(
                        SidebarGroup::new("Projects")
                            .child(SidebarMenu::new().children(project_menu_items)),
                    )
                    .when(!standalone_items.is_empty(), |this| {
                        this.child(
                            SidebarGroup::new("Standalone")
                                .child(SidebarMenu::new().children(standalone_items)),
                        )
                    })
                    .footer(
                        SidebarFooter::new().child(
                            h_flex()
                                .id("theme-select-footer")
                                .gap_2()
                                .items_center()
                                .child(IconName::Palette)
                                .child(
                                    Select::new(&self.theme_select)
                                        .placeholder("Theme")
                                        .w_full()
                                        .menu_max_h(rems(14.)),
                                )
                                .w_full(),
                        ),
                    ),
            )
    }
}

impl SidebarView {
    fn render_board_item(
        &self,
        board: &BoardDTO,
        cx: &mut Context<Self>,
        search_matches: bool,
    ) -> SidebarMenuItem {
        let board_id = board.id;
        let project_id = board.project_id;
        let title = board.title.clone();
        let is_active = self.active_item == Some(ActiveItem::Board(board_id));
        let is_renaming = self.renaming_board == Some(board_id);
        let projects = self
            .projects
            .iter()
            .map(|project| (project.id, project.name.clone()))
            .collect::<Vec<_>>();

        SidebarMenuItem::new(title.clone())
            .icon(IconName::LayoutDashboard)
            .when(!search_matches, |this| this.disable(true))
            .when(is_renaming, |this| {
                let input = self.rename_board_input.clone();
                this.suffix(move |_window, cx| {
                    Input::new(&input)
                        .small()
                        .bg(cx.theme().sidebar.opacity(0.))
                        .rounded_none()
                        .focus_bordered(false)
                        .border_0()
                        .text_xs()
                        .w_full()
                })
            })
            .active(is_active)
            .context_menu(move |mut menu, _, cx| {
                let muted = cx.theme().muted_foreground;
                menu = menu
                    .menu_element(Box::new(EditBoardAction(board_id)), move |_window, _cx| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child("Edit")
                            .child(Icon::new(IconName::Replace).xsmall().text_color(muted))
                    })
                    .menu_element(
                        Box::new(MoveBoardAction {
                            board_id,
                            project_id: None,
                        }),
                        move |_window, _cx| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child("Move to Standalone")
                                .child(Icon::new(IconName::Folder).xsmall().text_color(muted))
                        },
                    );

                for (target_project_id, name) in projects.clone() {
                    if Some(target_project_id) == project_id {
                        continue;
                    }
                    menu = menu.menu_element(
                        Box::new(MoveBoardAction {
                            board_id,
                            project_id: Some(target_project_id),
                        }),
                        move |_window, _cx| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child(format!("Move to {}", name))
                                .child(Icon::new(IconName::FolderOpen).xsmall().text_color(muted))
                        },
                    );
                }

                menu.menu_element(
                    Box::new(DeleteBoardAction(board_id)),
                    move |_window, _cx| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child("Delete")
                            .child(Icon::new(IconName::Delete).xsmall().text_color(muted))
                    },
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_board(board_id, project_id, title.clone(), cx);
                this.focus_handle.focus(window, cx);
                cx.notify();
            }))
    }

    fn render_note_item(
        &self,
        note: &NoteDTO,
        cx: &mut Context<Self>,
        search_matches: bool,
    ) -> SidebarMenuItem {
        let note_id = note.id;
        let project_id = note.project_id;
        let title = note.title.clone();
        let is_active = self.active_item == Some(ActiveItem::Note(note_id));
        let is_renaming = self.renaming_note == Some(note_id);
        let projects = self
            .projects
            .iter()
            .map(|project| (project.id, project.name.clone()))
            .collect::<Vec<_>>();

        SidebarMenuItem::new(title.clone())
            .icon(IconName::BookOpen)
            .when(!search_matches, |this| this.disable(true))
            .when(is_renaming, |this| {
                let input = self.rename_note_input.clone();
                this.suffix(move |_window, cx| {
                    Input::new(&input)
                        .small()
                        .bg(cx.theme().sidebar.opacity(0.))
                        .rounded_none()
                        .focus_bordered(false)
                        .border_0()
                        .text_xs()
                        .w_full()
                })
            })
            .active(is_active)
            .context_menu(move |mut menu, _, cx| {
                let muted = cx.theme().muted_foreground;
                menu = menu
                    .menu_element(Box::new(EditNoteAction(note_id)), move |_window, _cx| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .justify_between()
                            .child("Edit")
                            .child(Icon::new(IconName::Replace).xsmall().text_color(muted))
                    })
                    .menu_element(
                        Box::new(MoveNoteAction {
                            note_id,
                            project_id: None,
                        }),
                        move |_window, _cx| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child("Move to Standalone")
                                .child(Icon::new(IconName::Folder).xsmall().text_color(muted))
                        },
                    );

                for (target_project_id, name) in projects.clone() {
                    if Some(target_project_id) == project_id {
                        continue;
                    }
                    menu = menu.menu_element(
                        Box::new(MoveNoteAction {
                            note_id,
                            project_id: Some(target_project_id),
                        }),
                        move |_window, _cx| {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child(format!("Move to {}", name))
                                .child(Icon::new(IconName::FolderOpen).xsmall().text_color(muted))
                        },
                    );
                }

                menu.menu_element(Box::new(DeleteNoteAction(note_id)), move |_window, _cx| {
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_center()
                        .justify_between()
                        .child("Delete")
                        .child(Icon::new(IconName::Delete).xsmall().text_color(muted))
                })
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_note(note_id, project_id, title.clone(), cx);
                this.focus_handle.focus(window, cx);
                cx.notify();
            }))
    }
}
