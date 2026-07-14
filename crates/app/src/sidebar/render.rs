use std::rc::Rc;

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Collapsible, Icon, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    menu::PopupMenu,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarGroup, SidebarHeader, SidebarItem, SidebarMenuItem,
    },
    v_flex,
};

use super::content_item::SidebarContentItem;
use super::drag::{SidebarDragInfo, SidebarDragKind};
use super::event::SidebarEvent;
use super::{SidebarView, action::*};

#[derive(Clone)]
struct DraggableContentItem {
    menu_item: SidebarMenuItem,
    drag_info: SidebarDragInfo,
}

#[derive(Clone)]
struct DraggableProjectItem {
    project_id: u32,
    project_index: usize,
    name: SharedString,
    default_open: bool,
    active: bool,
    is_renaming: bool,
    rename_input: Entity<gpui_component::input::InputState>,
    is_first: bool,
    is_last: bool,
    children: Vec<DraggableContentItem>,
    drag_info: SidebarDragInfo,
    sidebar: Entity<SidebarView>,
}

#[derive(Clone)]
enum SidebarDragMenuEntry {
    Content(Box<DraggableContentItem>),
    Project(Box<DraggableProjectItem>),
}

#[derive(Clone)]
struct SidebarDragMenu {
    entries: Vec<SidebarDragMenuEntry>,
    collapsed: bool,
    standalone_drop_target: bool,
    sidebar: Entity<SidebarView>,
}

impl SidebarDragMenu {
    fn new(entries: Vec<SidebarDragMenuEntry>, sidebar: Entity<SidebarView>) -> Self {
        Self {
            entries,
            collapsed: false,
            standalone_drop_target: false,
            sidebar,
        }
    }

    fn standalone_drop_target(mut self) -> Self {
        self.standalone_drop_target = true;
        self
    }
}

impl Collapsible for SidebarDragMenu {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl SidebarItem for SidebarDragMenu {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let id = id.into();
        let is_empty = self.entries.is_empty();
        let sidebar = self.sidebar.clone();

        v_flex()
            .id(id.clone())
            .gap_1()
            .when(is_empty && self.standalone_drop_target, |this| {
                this.child(
                    h_flex()
                        .h_7()
                        .px_2()
                        .gap_2()
                        .rounded(cx.theme().radius)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .opacity(0.72)
                        .child(Icon::new(IconName::Folder).xsmall())
                        .child("Drop notes or boards here"),
                )
            })
            .children(self.entries.into_iter().enumerate().map(|(index, entry)| {
                entry.render(format!("{}-{index}", id), self.collapsed, window, cx)
            }))
            .when(self.standalone_drop_target, |this| {
                this.can_drop(|value, _, _| {
                    value
                        .downcast_ref::<SidebarDragInfo>()
                        .is_some_and(SidebarDragInfo::can_drop_on_standalone)
                })
                .drag_over::<SidebarDragInfo>(|this, _, _, cx| {
                    this.rounded(cx.theme().radius).bg(cx.theme().drop_target)
                })
                .on_drop(move |info: &SidebarDragInfo, _, cx| {
                    let SidebarDragKind::Content(item) = &info.kind else {
                        return;
                    };
                    sidebar.update(cx, |this, cx| item.move_to(this, None, cx));
                })
            })
            .into_any_element()
    }
}

impl SidebarDragMenuEntry {
    fn render(
        self,
        id: impl Into<ElementId>,
        collapsed: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        match self {
            Self::Content(item) => item.render(id, collapsed, window, cx),
            Self::Project(item) => item.render(id, collapsed, window, cx),
        }
    }
}

impl DraggableContentItem {
    fn render(
        self,
        id: impl Into<ElementId>,
        collapsed: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let id = id.into();
        let drag_info = self.drag_info.clone();

        div()
            .id(id.clone())
            .cursor_move()
            .on_drag(drag_info, |info, position, _, cx| {
                cx.new(|_| info.clone().position(position))
            })
            .child(
                self.menu_item
                    .collapsed(collapsed)
                    .render(format!("{}-row", id), window, cx),
            )
            .into_any_element()
    }
}

impl DraggableProjectItem {
    fn render(
        self,
        id: impl Into<ElementId>,
        collapsed: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let id = id.into();
        let open_state = window.use_keyed_state(
            format!("sidebar-project-open-{}", self.project_id),
            cx,
            |_, _| self.default_open,
        );
        let is_open = !collapsed && *open_state.read(cx);
        let project_id = self.project_id;
        let project_index = self.project_index;
        let sidebar = self.sidebar.clone();
        let click_sidebar = self.sidebar.clone();
        let click_open_state = open_state.clone();
        let caret_open_state = open_state.clone();
        let drop_open_state = open_state.clone();
        let rename_input = self.rename_input.clone();
        let is_renaming = self.is_renaming;
        let drag_info = self.drag_info.clone();
        let row = SidebarMenuItem::new(self.name)
            .icon(IconName::FolderOpen)
            .active(self.active)
            .suffix(move |_window, cx| {
                let caret_open_state = caret_open_state.clone();
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .justify_end()
                    .when(is_renaming, |this| {
                        this.child(
                            Input::new(&rename_input)
                                .small()
                                .bg(cx.theme().sidebar.opacity(0.))
                                .rounded_none()
                                .focus_bordered(false)
                                .border_0()
                                .text_xs()
                                .w_full(),
                        )
                    })
                    .child(
                        Button::new(("sidebar-project-caret", project_id as usize))
                            .xsmall()
                            .ghost()
                            .icon(
                                Icon::new(IconName::ChevronRight)
                                    .size_4()
                                    .when(is_open, |this| this.rotate(percentage(90. / 360.))),
                            )
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                caret_open_state.update(cx, |open, cx| {
                                    *open = !*open;
                                    cx.notify();
                                });
                            }),
                    )
            })
            .context_menu(move |menu, _, cx| {
                let muted = cx.theme().muted_foreground;
                menu.menu_element(
                    Box::new(RenameProjectAction(project_id)),
                    move |_window, _cx| {
                        SidebarView::render_context_menu_row("Rename", IconName::Replace, muted)
                    },
                )
                .when(!self.is_first, |menu| {
                    menu.menu_element(
                        Box::new(MoveProjectUpAction(project_id)),
                        move |_window, _cx| {
                            SidebarView::render_context_menu_row(
                                "Move up",
                                IconName::ArrowUp,
                                muted,
                            )
                        },
                    )
                })
                .when(!self.is_last, |menu| {
                    menu.menu_element(
                        Box::new(MoveProjectDownAction(project_id)),
                        move |_window, _cx| {
                            SidebarView::render_context_menu_row(
                                "Move down",
                                IconName::ArrowDown,
                                muted,
                            )
                        },
                    )
                })
                .menu_element(
                    Box::new(DeleteProjectAction(project_id)),
                    move |_window, _cx| {
                        SidebarView::render_context_menu_row(
                            "Move to Trash",
                            IconName::Delete,
                            muted,
                        )
                    },
                )
            })
            .on_click(move |_, window, cx| {
                click_open_state.update(cx, |open, cx| {
                    *open = !*open;
                    cx.notify();
                });
                click_sidebar.update(cx, |this, cx| {
                    this.active_project_id = Some(project_id);
                    cx.emit(SidebarEvent::ActivateProject { project_id });
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                });
            });

        v_flex()
            .id(id.clone())
            .gap_1()
            .child(
                div()
                    .id(format!("{}-drop-row", id))
                    .cursor_move()
                    .can_drop(move |value, _, _| {
                        value
                            .downcast_ref::<SidebarDragInfo>()
                            .is_some_and(|info| info.can_drop_on_project(project_id))
                    })
                    .drag_over::<SidebarDragInfo>(move |this, info, _, cx| match &info.kind {
                        SidebarDragKind::Project { source_index, .. } => {
                            if *source_index < project_index {
                                this.border_b_2()
                            } else {
                                this.border_t_2()
                            }
                            .border_color(cx.theme().primary)
                            .bg(cx.theme().drop_target.opacity(0.72))
                        }
                        SidebarDragKind::Content(_) => {
                            this.rounded(cx.theme().radius).bg(cx.theme().drop_target)
                        }
                    })
                    .on_drop(move |info: &SidebarDragInfo, _, cx| {
                        if matches!(&info.kind, SidebarDragKind::Content(_)) {
                            drop_open_state.update(cx, |open, cx| {
                                *open = true;
                                cx.notify();
                            });
                        }
                        sidebar.update(cx, |this, cx| match &info.kind {
                            SidebarDragKind::Project { id, .. } => {
                                this.reorder_project(*id, project_id, cx)
                            }
                            SidebarDragKind::Content(item) => {
                                item.move_to(this, Some(project_id), cx)
                            }
                        });
                    })
                    .on_drag(drag_info, |info, position, _, cx| {
                        cx.new(|_| info.clone().position(position))
                    })
                    .child(
                        row.collapsed(collapsed)
                            .render(format!("{}-row", id), window, cx),
                    ),
            )
            .when(is_open, |this| {
                this.child(
                    v_flex()
                        .id(format!("{}-children", id))
                        .ml_3p5()
                        .pl_2p5()
                        .py_0p5()
                        .gap_1()
                        .border_l_1()
                        .border_color(cx.theme().sidebar_border)
                        .children(self.children.into_iter().enumerate().map(|(index, child)| {
                            child.render(format!("{}-child-{index}", id), collapsed, window, cx)
                        })),
                )
            })
            .into_any_element()
    }
}

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

    fn render_draggable_content_item(
        &self,
        item: SidebarContentItem,
        origin: SharedString,
        cx: &mut Context<Self>,
        projects: Rc<Vec<(u32, SharedString)>>,
    ) -> DraggableContentItem {
        let drag_info = SidebarDragInfo::content(item.clone(), origin);
        let menu_item = self.render_content_item(item, cx, true, projects);

        DraggableContentItem {
            menu_item,
            drag_info,
        }
    }

    fn render_item_context_menu(
        mut menu: PopupMenu,
        item: SidebarContentItem,
        projects: Rc<Vec<(u32, SharedString)>>,
        cx: &mut App,
    ) -> PopupMenu {
        let muted = cx.theme().muted_foreground;
        menu = menu
            .menu_element(item.pin_action(), {
                let label = if item.is_pinned() { "Unpin" } else { "Pin" };
                move |_window, _cx| Self::render_context_menu_row(label, IconName::Star, muted)
            })
            .menu_element(item.edit_action(), move |_window, _cx| {
                Self::render_context_menu_row("Rename", IconName::Replace, muted)
            });

        if item.can_move_to(None) {
            menu = menu.menu_element(item.move_action(None), move |_window, _cx| {
                Self::render_context_menu_row("Move to Standalone", IconName::Folder, muted)
            });
        }

        for (target_project_id, name) in projects.iter() {
            if !item.can_move_to(Some(*target_project_id)) {
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
            Self::render_context_menu_row("Move to Trash", IconName::Delete, muted)
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
        let theme_name = theme.theme_name().clone();
        let search_query = self.search_input.read(cx).text().to_string();
        let search_lower = search_query.to_lowercase();
        let move_project_targets: Rc<Vec<(u32, SharedString)>> = Rc::new(
            self.projects
                .iter()
                .map(|project| (project.id, project.name.clone()))
                .collect(),
        );
        let sidebar = cx.entity();
        let mut pinned_items = Vec::new();
        for project in &self.projects {
            let origin = project.name.clone();
            pinned_items.extend(
                project
                    .notes
                    .iter()
                    .filter(|note| note.is_pinned)
                    .map(|note| {
                        SidebarDragMenuEntry::Content(Box::new(self.render_draggable_content_item(
                            note.into(),
                            origin.clone(),
                            cx,
                            move_project_targets.clone(),
                        )))
                    }),
            );
            pinned_items.extend(project.boards.iter().filter(|board| board.is_pinned).map(
                |board| {
                    SidebarDragMenuEntry::Content(Box::new(self.render_draggable_content_item(
                        board.into(),
                        origin.clone(),
                        cx,
                        move_project_targets.clone(),
                    )))
                },
            ));
        }
        pinned_items.extend(
            self.standalone_notes
                .iter()
                .filter(|note| note.is_pinned)
                .map(|note| {
                    SidebarDragMenuEntry::Content(Box::new(self.render_draggable_content_item(
                        note.into(),
                        "Standalone".into(),
                        cx,
                        move_project_targets.clone(),
                    )))
                }),
        );
        pinned_items.extend(
            self.standalone_boards
                .iter()
                .filter(|board| board.is_pinned)
                .map(|board| {
                    SidebarDragMenuEntry::Content(Box::new(self.render_draggable_content_item(
                        board.into(),
                        "Standalone".into(),
                        cx,
                        move_project_targets.clone(),
                    )))
                }),
        );

        let project_menu_items: Vec<SidebarDragMenuEntry> = self
            .projects
            .iter()
            .enumerate()
            .filter_map(|(project_index, project)| {
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
                let project_name = project.name.clone();
                let is_renaming = self.renaming_project == Some(project_id);
                let is_first_project = self
                    .projects
                    .first()
                    .map(|project| project.id == project_id)
                    .unwrap_or(false);
                let is_last_project = self
                    .projects
                    .last()
                    .map(|project| project.id == project_id)
                    .unwrap_or(false);
                let mut children = Vec::new();

                children.extend(filtered_notes.into_iter().map(|note| {
                    self.render_draggable_content_item(
                        note.into(),
                        project_name.clone(),
                        cx,
                        move_project_targets.clone(),
                    )
                }));

                children.extend(filtered_boards.into_iter().map(|board| {
                    self.render_draggable_content_item(
                        board.into(),
                        project_name.clone(),
                        cx,
                        move_project_targets.clone(),
                    )
                }));

                Some(SidebarDragMenuEntry::Project(Box::new(
                    DraggableProjectItem {
                        project_id,
                        project_index,
                        name: project_name.clone(),
                        default_open: project.is_expanded || !search_lower.is_empty(),
                        active: self.active_project_id == Some(project_id)
                            && self.active_item.is_none(),
                        is_renaming,
                        rename_input: self.rename_project_input.clone(),
                        is_first: is_first_project,
                        is_last: is_last_project,
                        drag_info: SidebarDragInfo::project(
                            project_id,
                            project_index,
                            project_name,
                            project.notes.len() + project.boards.len(),
                        ),
                        children,
                        sidebar: sidebar.clone(),
                    },
                )))
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

        let mut standalone_items: Vec<SidebarDragMenuEntry> = standalone_notes
            .into_iter()
            .map(|note| {
                SidebarDragMenuEntry::Content(Box::new(self.render_draggable_content_item(
                    note.into(),
                    "Standalone".into(),
                    cx,
                    move_project_targets.clone(),
                )))
            })
            .collect();

        standalone_items.extend(standalone_boards.into_iter().map(|board| {
            SidebarDragMenuEntry::Content(Box::new(self.render_draggable_content_item(
                board.into(),
                "Standalone".into(),
                cx,
                move_project_targets.clone(),
            )))
        }));
        let show_standalone = search_lower.is_empty() || !standalone_items.is_empty();

        div()
            .h_full()
            .flex_shrink_0()
            .on_action(cx.listener(Self::on_delete_board_action))
            .on_action(cx.listener(Self::on_edit_board_action))
            .on_action(cx.listener(Self::on_move_board_action))
            .on_action(cx.listener(Self::on_move_note_action))
            .on_action(cx.listener(Self::on_delete_note_action))
            .on_action(cx.listener(Self::on_edit_note_action))
            .on_action(cx.listener(Self::on_rename_project_action))
            .on_action(cx.listener(Self::on_delete_project_action))
            .on_action(cx.listener(Self::on_toggle_board_pinned_action))
            .on_action(cx.listener(Self::on_toggle_note_pinned_action))
            .on_action(cx.listener(Self::on_move_project_up_action))
            .on_action(cx.listener(Self::on_move_project_down_action))
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
                                SidebarHeader::new().child(
                                    v_flex()
                                        .id("header-title")
                                        .w_full()
                                        .gap_1()
                                        .flex_1()
                                        .overflow_hidden()
                                        .child(
                                            h_flex()
                                                .gap(px(3.))
                                                .items_end()
                                                .child(
                                                    div()
                                                        .font_family("Georgia")
                                                        .text_size(px(25.))
                                                        .line_height(relative(1.))
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_color(theme.sidebar_foreground)
                                                        .child("Castle"),
                                                )
                                                .child(
                                                    div()
                                                        .w(px(10.))
                                                        .h(px(22.))
                                                        .mb(px(1.))
                                                        .rounded(px(1.))
                                                        .bg(theme.primary),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .text_size(px(11.))
                                                .line_height(relative(1.2))
                                                .whitespace_nowrap()
                                                .child("notes  for  thoughtful  work")
                                                .text_color(theme.muted_foreground)
                                                .opacity(0.82),
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
                    .when(!pinned_items.is_empty(), |this| {
                        this.child(
                            SidebarGroup::new("Pinned")
                                .child(SidebarDragMenu::new(pinned_items, sidebar.clone())),
                        )
                    })
                    .child(
                        SidebarGroup::new("Projects")
                            .child(SidebarDragMenu::new(project_menu_items, sidebar.clone())),
                    )
                    .when(show_standalone, |this| {
                        this.child(
                            SidebarGroup::new("Standalone").child(
                                SidebarDragMenu::new(standalone_items, sidebar)
                                    .standalone_drop_target(),
                            ),
                        )
                    })
                    .footer(
                        v_flex()
                            .id("sidebar-utility-dock")
                            .w_full()
                            .gap_1()
                            .p_1()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme.sidebar_border.opacity(0.72))
                            .bg(theme.secondary.opacity(0.3))
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .id("sidebar-home")
                                            .flex_1()
                                            .h_9()
                                            .px_2()
                                            .gap_2()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(6.))
                                            .cursor_pointer()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.sidebar_foreground.opacity(0.82))
                                            .hover(|this| {
                                                this.bg(theme.secondary_hover.opacity(0.8))
                                                    .text_color(theme.sidebar_foreground)
                                            })
                                            .active(|this| {
                                                this.bg(theme.secondary_hover.opacity(0.95))
                                            })
                                            .child(
                                                Icon::new(IconName::LayoutDashboard)
                                                    .xsmall()
                                                    .text_color(theme.muted_foreground),
                                            )
                                            .child("Home")
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                cx.emit(SidebarEvent::OpenHome);
                                            })),
                                    )
                                    .child(
                                        h_flex()
                                            .id("sidebar-trash")
                                            .flex_1()
                                            .h_9()
                                            .px_2()
                                            .gap_2()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(6.))
                                            .cursor_pointer()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.sidebar_foreground.opacity(0.82))
                                            .hover(|this| {
                                                this.bg(theme.secondary_hover.opacity(0.8))
                                                    .text_color(theme.sidebar_foreground)
                                            })
                                            .active(|this| {
                                                this.bg(theme.secondary_hover.opacity(0.95))
                                            })
                                            .child(
                                                Icon::new(IconName::Delete)
                                                    .xsmall()
                                                    .text_color(theme.muted_foreground),
                                            )
                                            .child("Trash")
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                cx.emit(SidebarEvent::OpenTrash);
                                            })),
                                    ),
                            )
                            .child(div().mx_2().h(px(1.)).bg(theme.sidebar_border.opacity(0.6)))
                            .child(
                                h_flex()
                                    .id("theme-select-footer")
                                    .w_full()
                                    .h_9()
                                    .gap_2()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.))
                                    .px_2()
                                    .cursor_pointer()
                                    .hover(|this| {
                                        this.bg(theme.secondary_hover.opacity(0.8))
                                            .text_color(theme.sidebar_foreground)
                                    })
                                    .active(|this| this.bg(theme.secondary_hover.opacity(0.95)))
                                    .child(
                                        Icon::new(IconName::Palette)
                                            .xsmall()
                                            .text_color(theme.muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.sidebar_foreground.opacity(0.82))
                                            .truncate()
                                            .child(theme_name),
                                    )
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(SidebarEvent::OpenThemeSwitcher);
                                    })),
                            ),
                    ),
            )
    }
}
