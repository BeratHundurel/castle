use std::rc::Rc;

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, h_flex,
    input::Input,
    menu::PopupMenu,
    select::Select,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem,
    },
    v_flex,
};

use super::SidebarView;
use super::content_item::SidebarContentItem;
use super::event::SidebarEvent;

impl EventEmitter<SidebarEvent> for SidebarView {}

impl Focusable for SidebarView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SidebarView {
    fn render_content_item(
        &self,
        item: SidebarContentItem,
        cx: &mut Context<Self>,
        search_matches: bool,
        projects: Rc<Vec<(u32, SharedString)>>,
    ) -> SidebarMenuItem {
        let title = item.title();
        let is_active = self.active_item == Some(item.active_item());
        let is_renaming = item.is_renaming(self);
        let context_item = item.clone();
        let click_item = item.clone();

        SidebarMenuItem::new(title.clone())
            .icon(item.icon())
            .when(!search_matches, |this| this.disable(true))
            .when(is_renaming, |this| {
                let input = item.rename_input(self);
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
            .context_menu(move |menu, _, cx| {
                Self::render_item_context_menu(menu, context_item.clone(), projects.clone(), cx)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                click_item.select(this, cx);
                this.focus_handle.focus(window, cx);
                cx.notify();
            }))
    }

    fn render_item_context_menu(
        mut menu: PopupMenu,
        item: SidebarContentItem,
        projects: Rc<Vec<(u32, SharedString)>>,
        cx: &mut App,
    ) -> PopupMenu {
        let muted = cx.theme().muted_foreground;
        menu = menu
            .menu_element(item.edit_action(), move |_window, _cx| {
                Self::render_context_menu_row("Edit", IconName::Replace, muted)
            })
            .menu_element(item.move_action(None), move |_window, _cx| {
                Self::render_context_menu_row("Move to Standalone", IconName::Folder, muted)
            });

        for (target_project_id, name) in projects.iter() {
            if Some(*target_project_id) == item.project_id() {
                continue;
            }

            let target_project_id = *target_project_id;
            let name = name.clone();
            menu = menu.menu_element(
                item.move_action(Some(target_project_id)),
                move |_window, _cx| {
                    Self::render_context_menu_row(
                        format!("Move to {}", name),
                        IconName::FolderOpen,
                        muted,
                    )
                },
            );
        }

        menu.menu_element(item.delete_action(), move |_window, _cx| {
            Self::render_context_menu_row("Delete", IconName::Delete, muted)
        })
    }

    fn render_context_menu_row(
        label: impl Into<SharedString>,
        icon: IconName,
        color: Hsla,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .justify_between()
            .child(label.into())
            .child(Icon::new(icon).xsmall().text_color(color))
    }
}

impl Render for SidebarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let adding_board_to_project = self.adding_board_to_project;
        let search_query = self.search_input.read(cx).text().to_string();
        let search_lower = search_query.to_lowercase();
        let move_project_targets: Rc<Vec<(u32, SharedString)>> = Rc::new(
            self.projects
                .iter()
                .map(|project| (project.id, project.name.clone()))
                .collect(),
        );

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

                children.extend(filtered_notes.into_iter().map(|note| {
                    self.render_content_item(note.into(), cx, true, move_project_targets.clone())
                }));

                children.extend(filtered_boards.into_iter().map(|board| {
                    self.render_content_item(board.into(), cx, true, move_project_targets.clone())
                }));

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
                                    input.set_value("", window, cx);
                                    input.focus(window, cx);
                                });
                                cx.notify();
                            })),
                    );
                }

                Some(
                    SidebarMenuItem::new(project.name.clone())
                        .icon(IconName::FolderOpen)
                        .active(
                            self.active_project_id == Some(project_id)
                                && self.active_item.is_none(),
                        )
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
            .map(|note| {
                self.render_content_item(note.into(), cx, true, move_project_targets.clone())
            })
            .collect();

        standalone_items.extend(standalone_boards.into_iter().map(|board| {
            self.render_content_item(board.into(), cx, true, move_project_targets.clone())
        }));

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
                                                input.set_value("", window, cx);
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
                                .w_full()
                                .h_10()
                                .gap_2()
                                .justify_center()
                                .items_center()
                                .rounded(theme.radius)
                                .border_1()
                                .border_color(theme.border.opacity(0.7))
                                .bg(theme.secondary.opacity(0.55))
                                .px_2()
                                .hover(|this| {
                                    this.bg(theme.secondary_hover.opacity(0.8))
                                        .border_color(theme.primary.opacity(0.35))
                                })
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .size_6()
                                        .flex_shrink_0()
                                        .rounded(theme.radius)
                                        .bg(theme.primary.opacity(0.18))
                                        .text_color(theme.primary)
                                        .child(Icon::new(IconName::Palette).xsmall()),
                                )
                                .child(
                                    div().items_center().flex_1().child(
                                        Select::new(&self.theme_select)
                                            .placeholder("Theme")
                                            .appearance(false)
                                            .small()
                                            .w_full()
                                            .menu_max_h(rems(14.)),
                                    ),
                                ),
                        ),
                    ),
            )
    }
}
