use gpui::*;
use std::{collections::HashMap, path::PathBuf};

use gpui::{
    Action, App, AppContext, ClickEvent, Context, Entity, Focusable, MouseButton, ParentElement,
    Render, SharedString, Styled, Window, div, prelude::FluentBuilder, px, relative,
};

use gpui_component::{
    ActiveTheme, IconName, Root, Theme, ThemeRegistry,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    divider::Divider,
    h_flex,
    input::{Input, InputState},
    select::{SearchableVec, Select, SelectDelegate, SelectEvent, SelectState},
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};

#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = sidebar_story, no_json)]
pub struct SelectCompany(SharedString);

pub struct SidebarElement {
    active_items: HashMap<Item, bool>,
    last_active_item: Item,
    active_subitem: Option<SubItem>,
    focus_handle: gpui::FocusHandle,
    search_input: Entity<InputState>,
    theme_select: Entity<SelectState<SearchableVec<SharedString>>>,
}

impl SidebarElement {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut active_items = HashMap::new();
        active_items.insert(Item::Boards, true);

        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));

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
                if let Some(theme_name) = theme_name {
                    if let Some(theme_config) =
                        ThemeRegistry::global(cx).themes().get(theme_name).cloned()
                    {
                        Theme::global_mut(cx).apply_config(&theme_config);
                        cx.refresh_windows();
                    }
                }
            },
        )
        .detach();

        Self {
            active_items,
            last_active_item: Item::Boards,
            active_subitem: None,
            focus_handle: cx.focus_handle(),
            search_input,
            theme_select,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Item {
    Boards,
    Notes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubItem {
    Board1,
    Board2,
}

impl Item {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Boards => "Boards",
            Self::Notes => "Notes",
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            Self::Boards => IconName::SquareTerminal,
            Self::Notes => IconName::Bot,
        }
    }

    pub fn handler(
        &self,
    ) -> impl Fn(&mut SidebarElement, &ClickEvent, &mut Window, &mut Context<SidebarElement>) + 'static
    {
        let item = *self;
        move |this, _, window, cx| {
            this.last_active_item = item;
            this.active_subitem = None;
            this.focus_handle.focus(window, cx);

            cx.notify();
        }
    }

    pub fn items(&self) -> Vec<SubItem> {
        match self {
            Self::Boards => vec![SubItem::Board1, SubItem::Board2],
            Self::Notes => vec![],
        }
    }
}

impl SubItem {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Board1 => "ThemeSmith",
            Self::Board2 => "Printomi",
        }
    }

    pub fn handler(
        &self,
        item: &Item,
    ) -> impl Fn(&mut SidebarElement, &ClickEvent, &mut Window, &mut Context<SidebarElement>) + 'static
    {
        let item = *item;
        let subitem = *self;
        move |this, _, window, cx| {
            this.active_items.insert(item, true);
            this.last_active_item = item;
            this.active_subitem = Some(subitem);
            this.focus_handle.focus(window, cx);
            cx.notify();
        }
    }
}

impl Focusable for SidebarElement {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SidebarElement {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let groups: Vec<Item> = vec![Item::Boards, Item::Notes];

        h_flex()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .h_full()
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
                        SidebarGroup::new("Platform").child(SidebarMenu::new().children(
                            groups.iter().enumerate().map(|(_, item)| {
                                let is_active =
                                    self.last_active_item == *item && self.active_subitem == None;

                                SidebarMenuItem::new(item.label())
                                    .icon(item.icon())
                                    .active(is_active)
                                    .click_to_toggle(true)
                                    .collapsed(true)
                                    .children(item.items().into_iter().enumerate().map(
                                        |(_, sub_item)| {
                                            SidebarMenuItem::new(sub_item.label())
                                                .active(self.active_subitem == Some(sub_item))
                                                .on_click(cx.listener(sub_item.handler(&item)))
                                        },
                                    ))
                                    .on_click(cx.listener(item.handler()))
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
                v_flex()
                    .size_full()
                    .gap_4()
                    .p_4()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.focus_handle.focus(window, cx);
                            cx.notify();
                        }),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(Divider::vertical().h_4())
                            .child(
                                Breadcrumb::new()
                                    .child("Breadcrumb")
                                    .child(BreadcrumbItem::new("Home").on_click(cx.listener(
                                        |this, _, window, cx| {
                                            this.last_active_item = Item::Boards;
                                            this.focus_handle.focus(window, cx);
                                            cx.notify();
                                        },
                                    )))
                                    .child(
                                        BreadcrumbItem::new(self.last_active_item.label())
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.active_subitem = None;
                                                this.focus_handle.focus(window, cx);
                                                cx.notify();
                                            })),
                                    )
                                    .when_some(self.active_subitem, |this, subitem| {
                                        this.child(BreadcrumbItem::new(subitem.label()))
                                    }),
                            ),
                    ),
            )
    }
}

fn main() {
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        init_themes(cx);

        let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let view = SidebarElement::view(window, cx);
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
    let themes_dir = PathBuf::from("themes");
    if let Ok(entries) = std::fs::read_dir(&themes_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Err(err) =
                            ThemeRegistry::global_mut(cx).load_themes_from_str(&content)
                        {
                            eprintln!("Failed to load theme {:?}: {}", path, err);
                        }
                    }
                }
            }
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
