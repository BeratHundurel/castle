use gpui::*;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    App, AppContext, ClickEvent, Context, Entity, Focusable, MouseButton, ParentElement, Render,
    SharedString, Styled, Window, div, px, relative,
};

use gpui_component::{
    ActiveTheme, IconName, Root, Theme, ThemeRegistry,
    button::{Button, ButtonVariants},
    dialog::{
        Dialog, DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader,
        DialogTitle,
    },
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{SearchableVec, Select, SelectDelegate, SelectEvent, SelectState},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};

pub struct CastleApp {
    active_items: HashMap<u32, bool>,
    active_project_index: usize,
    active_board_index: Option<usize>,
    focus_handle: gpui::FocusHandle,
    search_input: Entity<InputState>,
    dialog_title_input: Entity<InputState>,
    dialog_description_input: Entity<InputState>,
    theme_select: Entity<SelectState<SearchableVec<SharedString>>>,
    projects: Vec<Project>,
}

impl CastleApp {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let projects = default_projects();
        let mut active_items = HashMap::new();
        active_items.insert(projects[0].id, true);

        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
        let dialog_title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Give your title"));

        let dialog_description_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Give your description")
                .multi_line(true)
                .rows(3)
                .auto_grow(1, 24)
                .soft_wrap(true)
                .searchable(true)
        });

        let registry = ThemeRegistry::global(cx);
        let themes: Vec<SharedString> = registry
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.clone())
            .collect();

        let current_theme = cx.theme().theme_name();
        let delegate = SearchableVec::new(themes);
        let selected_index = delegate
            .position(current_theme)
            .or_else(|| delegate.position(&SharedString::from("Alduin")));

        let theme_select =
            cx.new(|cx| SelectState::new(delegate, selected_index, window, cx).searchable(true));

        cx.subscribe(
            &theme_select,
            |_, _, event: &SelectEvent<SearchableVec<SharedString>>, cx| {
                let SelectEvent::Confirm(theme_name) = event;
                if let Some(theme_name) = theme_name
                    && let Some(theme_config) =
                        ThemeRegistry::global(cx).themes().get(theme_name).cloned()
                {
                    Theme::global_mut(cx).apply_config(&theme_config);
                    cx.refresh_windows();
                }
            },
        )
        .detach();

        Self {
            active_items,
            active_project_index: 0,
            active_board_index: Some(0),
            focus_handle: cx.focus_handle(),
            search_input,
            dialog_title_input,
            dialog_description_input,
            theme_select,
            projects,
        }
    }
}

struct Project {
    id: u32,
    name: String,
    boards: Vec<Board>,
}

impl Project {
    fn new(id: u32, name: &str, boards: Vec<Board>) -> Self {
        Self {
            id,
            name: name.to_string(),
            boards,
        }
    }

    pub fn handler(
        &self,
        index: usize,
    ) -> impl Fn(&mut CastleApp, &ClickEvent, &mut Window, &mut Context<CastleApp>) + 'static {
        move |app, _, window, cx| {
            app.active_project_index = index;
            app.active_board_index = None;
            app.focus_handle.focus(window, cx);
            cx.notify();
        }
    }
}

fn default_projects() -> Vec<Project> {
    let boards = default_boards();
    let p1_boards = boards
        .iter()
        .filter(|b| b.project_id == 1)
        .cloned()
        .collect();

    let p2_boards = boards
        .iter()
        .filter(|b| b.project_id == 2)
        .cloned()
        .collect();

    vec![
        Project::new(1, "ThemeSmith", p1_boards),
        Project::new(2, "Castle", p2_boards),
    ]
}

#[derive(Clone, PartialEq, Eq)]
struct DragInfo {
    entry_id: u32,
    source_board_id: u32,
    source_card_id: u32,
    position: Point<Pixels>,
    title: Arc<str>,
}

impl DragInfo {
    fn new(entry_id: u32, source_board_id: u32, source_card_id: u32, title: Arc<str>) -> Self {
        Self {
            entry_id,
            source_board_id,
            source_card_id,
            position: Point::default(),
            title,
        }
    }

    fn position(mut self, pos: Point<Pixels>) -> Self {
        self.position = pos;
        self
    }
}

impl Render for DragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let size = gpui::size(px(160.), px(40.));

        div()
            .pl(self.position.x - size.width.half())
            .pt(self.position.y - size.height.half())
            .child(
                div()
                    .flex()
                    .justify_start()
                    .items_center()
                    .w(size.width)
                    .h(size.height)
                    .p_2()
                    .bg(cx.theme().primary.opacity(0.7))
                    .text_color(cx.theme().primary_foreground)
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .shadow_md()
                    .child(self.title.clone().to_string()),
            )
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Card {
    id: u32,
    title: String,
    board_id: u32,
    drop_on: Option<DragInfo>,
    entries: Vec<Entry>,
}

#[derive(Clone, PartialEq, Eq)]
struct Board {
    id: u32,
    title: String,
    project_id: u32,
    cards: Vec<Card>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    id: u32,
    title: String,
    description: String,
}

impl Board {
    fn new(id: u32, title: &str, project_id: u32, cards: Vec<Card>) -> Self {
        Self {
            id,
            title: title.to_string(),
            project_id,
            cards,
        }
    }

    pub fn handler(
        &self,
        project_id: u32,
        project_index: usize,
        board_index: usize,
    ) -> impl Fn(&mut CastleApp, &ClickEvent, &mut Window, &mut Context<CastleApp>) + 'static {
        move |app, _, window, cx| {
            app.active_items.insert(project_id, true);
            app.active_project_index = project_index;
            app.active_board_index = Some(board_index);
            app.focus_handle.focus(window, cx);
            cx.notify();
        }
    }
}

impl Entry {
    fn new(id: u32, title: &str, description: &str) -> Self {
        Self {
            id,
            title: title.to_string(),
            description: description.to_string(),
        }
    }
}

impl Card {
    fn new(id: u32, title: &str, board_id: u32, entries: Vec<Entry>) -> Self {
        Self {
            id,
            title: title.to_string(),
            board_id,
            entries,
            drop_on: None,
        }
    }
}

fn default_cards_board_1() -> Vec<Card> {
    vec![
        Card::new(
            1,
            "To Do",
            1,
            vec![
                Entry::new(1, "Learn Rust", "Read ownership chapter"),
                Entry::new(2, "Build project", "Start Trello clone"),
            ],
        ),
        Card::new(
            2,
            "In Progress",
            1,
            vec![Entry::new(1, "API Design", "Define endpoints")],
        ),
        Card::new(
            3,
            "Done",
            1,
            vec![Entry::new(1, "Setup project", "Initialize cargo project")],
        ),
    ]
}

fn default_cards_board_2() -> Vec<Card> {
    vec![
        Card::new(
            1,
            "Backlog",
            2,
            vec![
                Entry::new(
                    1,
                    "Research competitors",
                    "Analyze similar task management apps",
                ),
                Entry::new(2, "Create wireframes", "Design initial UI mockups"),
                Entry::new(3, "Database planning", "Draft schema for users and boards"),
            ],
        ),
        Card::new(
            5,
            "New Card Test Scroll",
            2,
            vec![
                Entry::new(
                    1,
                    "Research competitors",
                    "Analyze similar task management apps",
                ),
                Entry::new(2, "Create wireframes", "Design initial UI mockups"),
                Entry::new(3, "Database planning", "Draft schema for users and boards"),
            ],
        ),
        Card::new(
            2,
            "In Development",
            2,
            vec![
                Entry::new(1, "Authentication system", "Implement JWT login flow"),
                Entry::new(
                    2,
                    "Kanban drag-and-drop",
                    "Enable moving tasks between boards",
                ),
            ],
        ),
        Card::new(
            3,
            "Testing",
            2,
            vec![Entry::new(
                1,
                "API integration tests",
                "Verify all endpoints work correctly",
            )],
        ),
        Card::new(
            4,
            "Completed",
            2,
            vec![
                Entry::new(
                    1,
                    "Project setup",
                    "Initialized Rust workspace and dependencies",
                ),
                Entry::new(2, "CI pipeline", "Configured GitHub Actions workflow"),
            ],
        ),
    ]
}

fn default_cards_board_3() -> Vec<Card> {
    vec![
        Card::new(
            1,
            "Ideas",
            3,
            vec![
                Entry::new(1, "New Theme", "Design a light theme"),
                Entry::new(2, "Refactoring", "Clean up CSS"),
            ],
        ),
        Card::new(
            2,
            "Reviewed",
            3,
            vec![Entry::new(1, "Dark Mode", "Approve dark mode palette")],
        ),
    ]
}

fn default_boards() -> Vec<Board> {
    vec![
        Board::new(1, "ThemeSmith Board", 1, default_cards_board_1()),
        Board::new(2, "Castle Board", 2, default_cards_board_2()),
        Board::new(3, "ThemeSmith Ideas", 1, default_cards_board_3()),
    ]
}

impl Focusable for CastleApp {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CastleApp {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);

        h_flex()
            .id("main-container")
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .size_full()
            .child(
                Sidebar::new("sidebar-story")
                    .w(px(260.))
                    .gap_0()
                    .header(
                        v_flex()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .child(
                                SidebarHeader::new()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(cx.theme().radius)
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .size_8()
                                            .flex_shrink_0()
                                            .child(IconName::GalleryVerticalEnd),
                                    )
                                    .child(
                                        v_flex()
                                            .gap_0()
                                            .text_sm()
                                            .flex_1()
                                            .line_height(relative(1.25))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child("Castle")
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().sidebar_foreground)
                                            .child(
                                                div()
                                                    .child("Your private note taking app")
                                                    .text_color(cx.theme().muted_foreground)
                                                    .text_xs(),
                                            ),
                                    ),
                            )
                            .child(
                                Input::new(&self.search_input)
                                    .cleanable(true)
                                    .prefix(IconName::Search),
                            ),
                    )
                    .child(
                        SidebarGroup::new("Projects").child(SidebarMenu::new().children(
                            self.projects.iter().enumerate().map(|(idx, item)| {
                                SidebarMenuItem::new(item.name.clone())
                                    .icon(IconName::FolderOpen)
                                    .active(
                                        self.active_project_index == idx
                                            && self.active_board_index.is_none(),
                                    )
                                    .default_open(self.active_project_index == idx)
                                    .click_to_toggle(true)
                                    .children(item.boards.iter().enumerate().map(
                                        |(b_idx, sub_item)| {
                                            SidebarMenuItem::new(sub_item.title.clone())
                                                .active(
                                                    self.active_project_index == idx
                                                        && self.active_board_index == Some(b_idx),
                                                )
                                                .on_click(cx.listener(
                                                    sub_item.handler(item.id, idx, b_idx),
                                                ))
                                        },
                                    ))
                                    .on_click(cx.listener(item.handler(idx)))
                            }),
                        )),
                    )
                    .footer(
                        SidebarFooter::new().justify_between().child(
                            h_flex()
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
            .child(
                h_flex()
                    .id("scrollable-container")
                    .size_full()
                    .overflow_x_scrollbar()
                    .gap_4()
                    .p_4()
                    .items_start()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.focus_handle.focus(window, cx);
                            cx.notify();
                        }),
                    )
                    .children({
                        let dialog_title_input = self.dialog_title_input.clone();
                        let dialog_description_input = self.dialog_description_input.clone();

                        let active_board =
                            self.projects.get(self.active_project_index).and_then(|p| {
                                if let Some(b_idx) = self.active_board_index {
                                    p.boards.get(b_idx)
                                } else {
                                    p.boards.first()
                                }
                            });

                        let board_id_for_render = active_board.map(|b| b.id).unwrap_or(0);

                        active_board
                            .map(|b| b.cards.as_slice())
                            .unwrap_or(&[])
                            .iter()
                            .map(move |card| {
                                let dialog_title_input = dialog_title_input.clone();
                                let dialog_description_input = dialog_description_input.clone();
                                let card_id = card.id;
                                let board_id = board_id_for_render;

                                v_flex()
                                    .id(card.id as usize)
                                    .gap_2()
                                    .w_80()
                                    .h_auto()
                                    .max_h_3_4()
                                    .overflow_y_scrollbar()
                                    .p_2()
                                    .bg(cx.theme().secondary)
                                    .text_color(cx.theme().foreground)
                                    .rounded(cx.theme().radius)
                                    .on_drop(cx.listener(move |this, info: &DragInfo, _, _| {
                                        if info.source_board_id == board_id
                                            && info.source_card_id == card_id
                                        {
                                            return;
                                        }

                                        let active_project =
                                            this.projects.get_mut(this.active_project_index);

                                        if let Some(project) = active_project {
                                            let mut moving_entry: Option<Entry> = None;

                                            if let Some(b) = project
                                                .boards
                                                .iter_mut()
                                                .find(|b| b.id == info.source_board_id)
                                                && let Some(source_card) = b
                                                    .cards
                                                    .iter_mut()
                                                    .find(|c| c.id == info.source_card_id)
                                                && let Some(index) = source_card
                                                    .entries
                                                    .iter()
                                                    .position(|entry| entry.id == info.entry_id)
                                            {
                                                moving_entry =
                                                    Some(source_card.entries.remove(index));
                                            }

                                            if let Some(entry) = moving_entry
                                                && let Some(b) = project
                                                    .boards
                                                    .iter_mut()
                                                    .find(|b| b.id == board_id)
                                                && let Some(c) =
                                                    b.cards.iter_mut().find(|c| c.id == card_id)
                                            {
                                                c.entries.push(entry);
                                                c.drop_on = Some(info.clone());
                                            }
                                        }
                                    }))
                                    .child(
                                        div()
                                            .p_1()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(card.title.clone()),
                                    )
                                    .children(card.entries.iter().map(|entry| {
                                        let drag_info = DragInfo::new(
                                            entry.id,
                                            board_id,
                                            card_id,
                                            entry.title.clone().into(),
                                        );

                                        div()
                                            .id(entry.id as usize)
                                            .p_2()
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .rounded(cx.theme().radius)
                                            .hover(|this| {
                                                this.bg(cx.theme().primary_hover)
                                                    .cursor(CursorStyle::PointingHand)
                                            })
                                            .cursor_move()
                                            .text_sm()
                                            .w_full()
                                            .child(entry.title.clone())
                                            .on_drag(
                                                drag_info,
                                                |info: &DragInfo, position, _, cx| {
                                                    cx.new(|_| info.clone().position(position))
                                                },
                                            )
                                    }))
                                    .child(
                                        div().w_full().child(
                                            Dialog::new(cx)
                                                .trigger(
                                                    h_flex()
                                                        .id("Add Item")
                                                        .w_full()
                                                        .rounded(cx.theme().radius)
                                                        .gap_2()
                                                        .p_1()
                                                        .text_color(cx.theme().secondary_foreground)
                                                        .text_sm()
                                                        .hover(|this| {
                                                            this.bg(cx.theme().secondary_hover)
                                                                .text_color(
                                                                    cx.theme().accent_foreground,
                                                                )
                                                                .cursor(CursorStyle::PointingHand)
                                                        })
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(IconName::Plus)
                                                        .child("Add a card"),
                                                )
                                                .title("Add a new entry")
                                                .content({
                                                    move |content, _, _| {
                                                        content
                                                    .child(
                                                        DialogHeader::new()
                                                            .child(
                                                                DialogTitle::new()
                                                                    .child("Add a new entry"),
                                                            )
                                                            .child(DialogDescription::new().child(
                                                                "Enter the information needed",
                                                            )),
                                                    )
                                                    .child(
                                                        v_flex()
                                                            .gap_2()
                                                            .child(Input::new(&dialog_title_input))
                                                            .child(Input::new(
                                                                &dialog_description_input,
                                                            )),
                                                    )
                                                    .child(
                                                        DialogFooter::new()
                                                            .justify_between()
                                                            .child(
                                                                DialogClose::new().child(
                                                                    Button::new("cancel")
                                                                        .label("Cancel")
                                                                        .outline(),
                                                                ),
                                                            )
                                                            .child(
                                                                DialogAction::new().child(
                                                                    Button::new("confirm")
                                                                        .primary()
                                                                        .label("Confirm"),
                                                                ),
                                                            ),
                                                    )
                                                    }
                                                }),
                                        ),
                                    )
                            })
                    }),
            )
            .children(dialog_layer)
    }
}

#[tokio::main]
async fn main() {
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        init_themes(cx);

        let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Maximized(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let view = CastleApp::view(window, cx);
                    // This first level on the window, should be a Root.
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Failed to open window");
        })
        .detach();
    });
}

fn init_themes(cx: &mut App) {
    let theme_contents = [
        include_str!("../themes/alduin.json"),
        include_str!("../themes/ayu.json"),
        include_str!("../themes/catppuccin.json"),
        include_str!("../themes/everforest.json"),
        include_str!("../themes/gruvbox.json"),
        include_str!("../themes/harper.json"),
        include_str!("../themes/jellybeans.json"),
        include_str!("../themes/molokai.json"),
        include_str!("../themes/tokyonight.json"),
        include_str!("../themes/twilight.json"),
    ];

    for content in theme_contents {
        if let Err(err) = ThemeRegistry::global_mut(cx).load_themes_from_str(content) {
            eprintln!("Failed to load embedded theme: {}", err);
        }
    }

    apply_default_theme(cx);
    cx.refresh_windows();
}

fn apply_default_theme(cx: &mut App) {
    let theme_name = SharedString::from("Alduin");
    if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
        Theme::global_mut(cx).apply_config(&theme);
    }
}
