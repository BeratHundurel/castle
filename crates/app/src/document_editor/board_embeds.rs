use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use gpui::{
    Context, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node},
    v_flex,
};

use crate::AppServices;

use super::DocumentEditorView;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct EmbedKey {
    board_id: i64,
    view_id: Option<i64>,
}

#[derive(Clone)]
pub(super) enum EmbedState {
    Loading,
    Available(Arc<storage::board_projection::BoardViewProjection>),
    MissingBoard,
    MissingView,
    Error(SharedString),
}

#[derive(Clone)]
struct EmbedBlock {
    key: EmbedKey,
    fallback_title: Option<String>,
    source_range: Range<usize>,
}

#[derive(Clone)]
pub(super) struct BoardViewEmbedPlugin {
    editor: Entity<DocumentEditorView>,
    states: Arc<HashMap<EmbedKey, EmbedState>>,
}

impl BoardViewEmbedPlugin {
    pub(super) fn new(
        editor: Entity<DocumentEditorView>,
        states: Arc<HashMap<EmbedKey, EmbedState>>,
    ) -> Self {
        Self { editor, states }
    }
}

impl MarkdownPlugin for BoardViewEmbedPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "castle-board-view"
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Code(code) = node else {
            return None;
        };
        if code.lang.as_deref() != Some("castle-board-view") {
            return None;
        }
        let (board_id, view_id, fallback_title) =
            storage::board_projection::parse_embed_config(&code.value).ok()?;
        let position = node.position()?;
        let block = EmbedBlock {
            key: EmbedKey { board_id, view_id },
            fallback_title,
            source_range: (cx.offset() + position.start.offset)
                ..(cx.offset() + position.end.offset),
        };
        Some(
            MarkdownNode::new(self.name(), block)
                .markdown(cx.node_source(node).unwrap_or_default()),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        let Some(block) = node.data::<EmbedBlock>() else {
            return div().into_any_element();
        };
        let state = self
            .states
            .get(&block.key)
            .cloned()
            .unwrap_or(EmbedState::Loading);
        match state {
            EmbedState::Available(projection) => render_projection(
                self.editor.clone(),
                projection,
                block.source_range.start,
                cx,
            ),
            EmbedState::Loading => render_status(
                self.editor.clone(),
                block.clone(),
                "Loading board view…",
                false,
                cx,
            ),
            EmbedState::MissingBoard => render_status(
                self.editor.clone(),
                block.clone(),
                "This board is unavailable",
                true,
                cx,
            ),
            EmbedState::MissingView => render_status(
                self.editor.clone(),
                block.clone(),
                "This saved view is unavailable",
                true,
                cx,
            ),
            EmbedState::Error(error) => render_status(
                self.editor.clone(),
                block.clone(),
                &format!("Could not load board view: {error}"),
                true,
                cx,
            ),
        }
    }
}

fn render_projection(
    editor: Entity<DocumentEditorView>,
    projection: Arc<storage::board_projection::BoardViewProjection>,
    occurrence: usize,
    cx: &mut gpui::App,
) -> gpui::AnyElement {
    let board_id = u32::try_from(projection.board_id).ok();
    let title = projection
        .view_name
        .as_ref()
        .map(|view| format!("{} · {view}", projection.board_title))
        .unwrap_or_else(|| format!("{} · All cards", projection.board_title));
    v_flex()
        .id(("castle-board-view", occurrence as u64))
        .w_full()
        .gap_2()
        .p_3()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary.opacity(0.2))
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .child(Icon::new(IconName::LayoutDashboard).xsmall())
                .child(
                    div()
                        .flex_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .when_some(board_id, |this, board_id| {
                    let editor = editor.clone();
                    this.child(
                        Button::new("open-board")
                            .label("Open board")
                            .ghost()
                            .small()
                            .on_click(move |_, _, cx| {
                                editor.update(cx, |_, cx| {
                                    cx.emit(super::DocumentEditorEvent::OpenWorkspaceTarget(
                                        crate::workspace_navigation::WorkspaceNavigationTarget::board(board_id),
                                    ));
                                });
                            }),
                    )
                }),
        )
        .children(projection.lists.iter().filter(|list| !list.cards.is_empty()).map(|list| {
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().muted_foreground)
                        .child(list.title.clone()),
                )
                .children(list.cards.iter().map(|card| {
                    let card_id = u32::try_from(card.id).ok();
                    let editor = editor.clone();
                    let mut metadata = Vec::new();
                    if projection
                        .visible_properties
                        .contains(&storage::board_properties::PropertyKey::DueDate)
                        && let Some(due_on) = card.due_on.as_ref()
                    {
                        metadata.push(due_on.clone());
                    }
                    if projection
                        .visible_properties
                        .contains(&storage::board_properties::PropertyKey::Labels)
                        && !card.labels.is_empty()
                    {
                        metadata.push(card.labels.join(", "));
                    }
                    if projection
                        .visible_properties
                        .contains(&storage::board_properties::PropertyKey::RelatedNotes)
                        && card.related_note_count > 0
                    {
                        metadata.push(format!("{} note(s)", card.related_note_count));
                    }
                    metadata.extend(
                        card.custom_properties
                            .iter()
                            .map(|(name, value)| format!("{name}: {value}")),
                    );
                    v_flex()
                        .id(("card", card.id as u64))
                        .gap_0p5()
                        .px_2()
                        .py(if projection.compact_cards {
                            gpui::px(4.)
                        } else {
                            gpui::px(7.)
                        })
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border.opacity(0.6))
                        .when(card_id.is_some() && board_id.is_some(), |this| {
                            this.cursor_pointer()
                                .hover(|this| this.bg(cx.theme().accent.opacity(0.35)))
                                .on_click(move |_, _, cx| {
                                    if let (Some(board_id), Some(card_id)) = (board_id, card_id) {
                                        editor.update(cx, |_, cx| {
                                            cx.emit(super::DocumentEditorEvent::OpenWorkspaceTarget(
                                                crate::workspace_navigation::WorkspaceNavigationTarget::card(
                                                    board_id, card_id,
                                                ),
                                            ));
                                        });
                                    }
                                })
                        })
                        .child(div().text_sm().child(card.title.clone()))
                        .when(!metadata.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(metadata.join(" · ")),
                            )
                        })
                }))
        }))
        .when(projection.matching_card_count == 0, |this| {
            this.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No cards match this view."),
            )
        })
        .when(projection.remaining_card_count > 0, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "+{} more card(s). Open the board to see everything.",
                        projection.remaining_card_count
                    )),
            )
        })
        .into_any_element()
}

fn render_status(
    editor: Entity<DocumentEditorView>,
    block: EmbedBlock,
    message: &str,
    actions: bool,
    cx: &mut gpui::App,
) -> gpui::AnyElement {
    let fallback = block
        .fallback_title
        .clone()
        .unwrap_or_else(|| format!("Board {}", block.key.board_id));
    v_flex()
        .id(("castle-board-view", block.source_range.start as u64))
        .w_full()
        .gap_2()
        .p_3()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(fallback))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(message.to_string()),
        )
        .when(actions, |this| {
            let replacement_editor = editor.clone();
            let replacement_range = block.source_range.clone();
            let removal_editor = editor.clone();
            let removal_range = block.source_range.clone();
            this.child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("replace")
                            .label("Choose replacement")
                            .ghost()
                            .small()
                            .on_click(move |_, window, cx| {
                                replacement_editor.update(cx, |editor, cx| {
                                    editor.select_source_range(
                                        replacement_range.clone(),
                                        window,
                                        cx,
                                    );
                                    cx.emit(super::DocumentEditorEvent::InsertBoardView {
                                        note_id: editor.note_id,
                                    });
                                });
                            }),
                    )
                    .child(
                        Button::new("remove")
                            .label("Remove embed")
                            .ghost()
                            .small()
                            .on_click(move |_, window, cx| {
                                removal_editor.update(cx, |editor, cx| {
                                    editor.replace_source_range(
                                        removal_range.clone(),
                                        "",
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    ),
            )
        })
        .into_any_element()
}

impl DocumentEditorView {
    pub(crate) fn refresh_board_embeds(&mut self, cx: &mut Context<Self>) {
        let content = self.editor.read(cx).value().to_string();
        let keys = storage::board_projection::parse_board_view_embeds(&content)
            .into_iter()
            .map(|embed| EmbedKey {
                board_id: embed.board_id,
                view_id: embed.view_id,
            })
            .collect::<HashSet<_>>();
        let mut states = self.embeds.states.as_ref().clone();
        states.retain(|key, _| keys.contains(key));
        let keys_to_load = keys
            .iter()
            .copied()
            .filter(|key| !states.contains_key(key))
            .collect::<HashSet<_>>();
        for key in &keys_to_load {
            states.insert(*key, EmbedState::Loading);
        }
        self.embeds.states = Arc::new(states);
        if keys.is_empty() {
            self.embeds.request.clear();
            self.embeds.loading_keys.clear();
            cx.notify();
            return;
        }
        if keys_to_load.is_empty() {
            cx.notify();
            return;
        }
        self.start_board_embed_load(keys_to_load, cx);
    }

    pub(crate) fn refresh_board_embeds_for(&mut self, board_id: i64, cx: &mut Context<Self>) {
        let content = self.editor.read(cx).value().to_string();
        let keys = storage::board_projection::parse_board_view_embeds(&content)
            .into_iter()
            .filter(|embed| embed.board_id == board_id)
            .map(|embed| EmbedKey {
                board_id: embed.board_id,
                view_id: embed.view_id,
            })
            .collect::<HashSet<_>>();
        if keys.is_empty() {
            return;
        }
        self.start_board_embed_load(keys, cx);
    }

    pub(crate) fn reload_board_embeds(&mut self, cx: &mut Context<Self>) {
        let content = self.editor.read(cx).value().to_string();
        let keys = storage::board_projection::parse_board_view_embeds(&content)
            .into_iter()
            .map(|embed| EmbedKey {
                board_id: embed.board_id,
                view_id: embed.view_id,
            })
            .collect::<HashSet<_>>();
        if !keys.is_empty() {
            self.start_board_embed_load(keys, cx);
        }
    }

    fn start_board_embed_load(&mut self, mut keys: HashSet<EmbedKey>, cx: &mut Context<Self>) {
        keys.extend(
            self.embeds
                .loading_keys
                .iter()
                .copied()
                .filter(|key| self.embeds.states.contains_key(key)),
        );
        self.embeds.loading_keys = keys.clone();
        let generation = self.embeds.request.begin();
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        let task = cx.spawn(async move |this, cx| {
            let (cancel_on_drop, cancelled) = tokio::sync::oneshot::channel::<()>();
            let load = runtime.spawn(async move {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    states = async move {
                        let mut states = HashMap::new();
                        for key in keys {
                            let state = match storage::board_projection::load_board_view_projection(
                                &db,
                                key.board_id,
                                key.view_id,
                            )
                            .await
                            {
                                Ok(storage::board_projection::BoardViewProjectionResult::Available(
                                    projection,
                                )) => EmbedState::Available(Arc::new(projection)),
                                Ok(storage::board_projection::BoardViewProjectionResult::MissingBoard) => {
                                    EmbedState::MissingBoard
                                }
                                Ok(storage::board_projection::BoardViewProjectionResult::MissingView) => {
                                    EmbedState::MissingView
                                }
                                Err(error) => EmbedState::Error(error.to_string().into()),
                            };
                            states.insert(key, state);
                        }
                        states
                    } => Some(states),
                }
            });
            let result = load.await;
            drop(cancel_on_drop);
            this.update(cx, |this, cx| {
                if this.embeds.request.generation() != generation {
                    return;
                }
                let loading_keys = std::mem::take(&mut this.embeds.loading_keys);
                let mut merged = this.embeds.states.as_ref().clone();
                match result {
                    Ok(Some(states)) => {
                        for (key, state) in states {
                            if merged.contains_key(&key) {
                                merged.insert(key, state);
                            }
                        }
                    }
                    Ok(None) => return,
                    Err(error) => {
                        for key in keys_for_error(&merged, &loading_keys) {
                            merged.insert(key, EmbedState::Error(error.to_string().into()));
                        }
                    }
                }
                this.embeds.states = Arc::new(merged);
                cx.notify();
            })
            .ok();
        });
        self.embeds.request.set_task(task);
        cx.notify();
    }
}

fn keys_for_error(
    states: &HashMap<EmbedKey, EmbedState>,
    loading: &HashSet<EmbedKey>,
) -> Vec<EmbedKey> {
    if loading.is_empty() {
        states.keys().copied().collect()
    } else {
        loading.iter().copied().collect()
    }
}
