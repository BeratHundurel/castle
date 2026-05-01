use gpui::*;
use std::{collections::HashMap, path::PathBuf};

use gpui::{
    Action, App, AppContext, ClickEvent, Context, Entity, Focusable, MouseButton, ParentElement,
    Render, SharedString, Styled, Window, div, px, relative,
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
    dialog_title_input: Entity<InputState>,
    dialog_description_input: Entity<InputState>,
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
        let dialog_title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Give your title"));

        let dialog_description_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Give your description")
                .multi_line(true)
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
            dialog_title_input,
            dialog_description_input,
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
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let groups: Vec<Item> = vec![Item::Boards, Item::Notes];
        let dialog_layer = Root::render_dialog_layer(window, cx);

        let dialog_title_input = self.dialog_title_input.clone();
        let dialog_description_input = self.dialog_description_input.clone();

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
                        SidebarGroup::new("Projects").child(SidebarMenu::new().children(
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
                h_flex()
                    .size_full()
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
                    .child(
                        v_flex()
                            .gap_2()
                            .w_80()
                            .p_2()
                            .bg(cx.theme().secondary)
                            .text_color(cx.theme().foreground)
                            .rounded(cx.theme().radius)
                            .child(div().p_1().font_weight(FontWeight::MEDIUM).child("TODO's"))
                            .child(
                                div()
                                    .id("First Item")
                                    .p_2()
                                    .bg(cx.theme().primary)
                                    .text_color(cx.theme().primary_foreground)
                                    .rounded(cx.theme().radius)
                                    .hover(|this| {
                                        this.bg(cx.theme().primary_hover)
                                            .cursor(CursorStyle::PointingHand)
                                    })
                                    .text_sm()
                                    .w_full()
                                    .child("First Item On the List"),
                            )
                            .child(
                                div()
                                    .id("Second Item")
                                    .p_2()
                                    .bg(cx.theme().primary)
                                    .text_color(cx.theme().primary_foreground)
                                    .text_sm()
                                    .rounded(cx.theme().radius)
                                    .hover(|this| {
                                        this.bg(cx.theme().primary_hover).cursor(CursorStyle::Arrow)
                                    })
                                    .w_full()
                                    .child("Second Item On the List"),
                            )
                            .child(
                                div()
                                    .id("Third Item")
                                    .p_2()
                                    .bg(cx.theme().primary)
                                    .text_color(cx.theme().primary_foreground)
                                    .rounded(cx.theme().radius)
                                    .hover(|this| {
                                        this.bg(cx.theme().primary_hover).cursor(CursorStyle::Arrow)
                                    })
                                    .text_sm()
                                    .w_full()
                                    .child("Third Item On the List"),
                            )
                            .child(
                                div().w_full().child(
                                    Dialog::new(cx)
                                        .trigger(
                                            h_flex()
                                                .id("Add Item")
                                                .w_full()
                                                .gap_2()
                                                .p_1()
                                                .text_color(cx.theme().secondary_foreground)
                                                .text_sm()
                                                .hover(|this| {
                                                    this.bg(cx.theme().secondary_hover)
                                                        .text_color(cx.theme().accent_foreground)
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
                            ),
                    ),
            )
            .children(dialog_layer)
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
                    window_bounds: Some(WindowBounds::Maximized(bounds)),
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
