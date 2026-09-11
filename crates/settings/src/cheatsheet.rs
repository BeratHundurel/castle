use std::collections::BTreeMap;

use gpui_kit::component::{
    ActiveTheme, Icon, IconName, StyledExt as _, h_flex,
    input::{Input, InputEvent, InputState},
    kbd::Kbd,
    scroll::ScrollableElement as _,
    v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div, rems,
};

use crate::{ShortcutReference, shortcuts::shortcut_context_name};

struct ShortcutEntry {
    shortcut: ShortcutReference,
    search_text: String,
}

struct ShortcutGroup {
    context: SharedString,
    title: SharedString,
    entries: Vec<ShortcutEntry>,
}

pub struct CheatsheetView {
    search: Entity<InputState>,
    groups: Vec<ShortcutGroup>,
}

impl CheatsheetView {
    pub fn view(
        shortcuts: Vec<ShortcutReference>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let search = cx.new(|cx| {
                InputState::new(window, cx).placeholder("Search commands, contexts, or keys…")
            });
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            })
            .detach();
            Self {
                search,
                groups: shortcut_groups(shortcuts),
            }
        })
    }
}

impl Focusable for CheatsheetView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search.focus_handle(cx)
    }
}

impl Render for CheatsheetView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).text().to_string().to_lowercase();
        let query = query.trim();
        let mut count = 0;
        let mut sections = Vec::new();
        for group in &self.groups {
            let entries = group
                .entries
                .iter()
                .filter(|entry| entry.matches(query))
                .collect::<Vec<_>>();
            if entries.is_empty() {
                continue;
            }
            count += entries.len();
            sections.push(
                v_flex()
                    .id(group.context.clone())
                    .w_full()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .pb_2()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(group.title.clone()),
                    )
                    .children(entries.into_iter().map(|entry| {
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .min_h(rems(2.25))
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .py_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .child(entry.shortcut.action.clone()),
                            )
                            .child(
                                h_flex().flex_shrink_0().gap_1().children(
                                    entry
                                        .shortcut
                                        .keystrokes
                                        .iter()
                                        .cloned()
                                        .map(|stroke| Kbd::new(stroke).outline()),
                                ),
                            )
                    })),
            );
        }

        let results = if sections.is_empty() {
            v_flex().gap_2().py_8().child("No shortcuts found").child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Try a command name, a context such as Markdown, or a key such as F1."),
            )
        } else {
            v_flex().gap_6().children(sections)
        };

        v_flex()
            .id("cheatsheet")
            .debug_selector(|| "cheatsheet".to_owned())
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .p_6()
                    .gap_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_xl().font_semibold().child("Cheatsheet"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Keyboard shortcuts, grouped by where they work."),
                            ),
                    )
                    .child(
                        Input::new(&self.search)
                            .prefix(Icon::new(IconName::Search))
                            .cleanable(true),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{count} shortcuts")),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .w_full()
                    .overflow_y_scrollbar()
                    .id("cheatsheet-scroll")
                    .child(div().w_full().p_6().child(results)),
            )
    }
}

impl ShortcutEntry {
    fn matches(&self, query: &str) -> bool {
        query
            .split_whitespace()
            .all(|word| self.search_text.contains(word))
    }
}

fn shortcut_groups(shortcuts: Vec<ShortcutReference>) -> Vec<ShortcutGroup> {
    let mut groups = BTreeMap::<SharedString, Vec<ShortcutEntry>>::new();
    for shortcut in shortcuts {
        let keys = shortcut
            .keystrokes
            .iter()
            .map(|stroke| format!("{} {}", Kbd::format(stroke), stroke.unparse()))
            .collect::<Vec<_>>()
            .join(" ");
        let search_text = format!(
            "{} {} {} {}",
            shortcut.action,
            shortcut.context,
            shortcut_context_name(&shortcut.context),
            keys,
        )
        .to_lowercase();
        groups
            .entry(shortcut.context.clone())
            .or_default()
            .push(ShortcutEntry {
                shortcut,
                search_text,
            });
    }
    groups
        .into_iter()
        .map(|(context, mut entries)| {
            entries.sort_by(|left, right| left.shortcut.action.cmp(&right.shortcut.action));
            ShortcutGroup {
                title: shortcut_context_name(&context),
                context,
                entries,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{CheatsheetView, shortcut_groups};
    use crate::ShortcutReference;
    use gpui_kit::{Focusable as _, Keystroke, TestAppContext, px, size};

    fn shortcuts() -> Vec<ShortcutReference> {
        vec![
            ShortcutReference {
                action: "Open Cheatsheet".into(),
                context: "AppShell".into(),
                keystrokes: vec![Keystroke::parse("f1").expect("valid binding")],
            },
            ShortcutReference {
                action: "Bold".into(),
                context: "MarkdownSource".into(),
                keystrokes: vec![Keystroke::parse("ctrl-b").expect("valid binding")],
            },
        ]
    }

    #[test]
    fn cheatsheet_search_matches_commands_contexts_and_keys() {
        let groups = shortcut_groups(shortcuts());
        assert_eq!(groups[0].title, "Application");
        assert_eq!(groups[1].title, "Markdown Source");
        assert!(groups[0].entries[0].matches("cheatsheet f1"));
        assert!(groups[0].entries[0].matches("application"));
        assert!(groups[1].entries[0].matches("markdown bold"));
        assert!(groups[1].entries[0].matches("ctrl-b"));
        assert!(groups[1].entries[0].matches(""));
        assert!(!groups[1].entries[0].matches("bold f1"));
        assert!(shortcut_groups(Vec::new()).is_empty());
    }

    #[gpui_kit::test]
    fn cheatsheet_search_uses_input_state_and_fits_the_viewport(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut view = None;
        let (root, cx) = cx.add_window_view(|window, cx| {
            let cheatsheet = CheatsheetView::view(shortcuts(), window, cx);
            view = Some(cheatsheet.clone());
            gpui_kit::component::Root::new(cheatsheet, window, cx)
        });
        let view = view.expect("cheatsheet exists");
        cx.update(|window, cx| view.focus_handle(cx).focus(window, cx));
        cx.simulate_input("  BOLD  ");
        view.read_with(cx, |view, cx| {
            let query = view
                .search
                .read(cx)
                .text()
                .to_string()
                .trim()
                .to_lowercase();
            let matches = view
                .groups
                .iter()
                .flat_map(|group| &group.entries)
                .filter(|entry| entry.matches(&query))
                .count();
            assert_eq!(matches, 1);
        });
        for (width, height) in [(480., 600.), (1_200., 800.)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            let bounds = cx.debug_bounds("cheatsheet").expect("cheatsheet renders");
            assert_eq!(bounds.size, size(px(width), px(height)));
        }
        drop(root);
    }
}
