use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
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

use runtime::AppRuntime;

use super::DocumentEditorView;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct EmbedTarget {
    board_id: i64,
    view_id: Option<i64>,
}

#[derive(Clone, Debug)]
pub(super) struct EmbedKey {
    raw_target: String,
    target: Option<EmbedTarget>,
}

impl EmbedKey {
    fn unresolved(raw_target: String) -> Self {
        Self {
            raw_target,
            target: None,
        }
    }

    fn resolved(raw_target: String, target: EmbedTarget) -> Self {
        Self {
            raw_target,
            target: Some(target),
        }
    }
}

impl PartialEq for EmbedKey {
    fn eq(&self, other: &Self) -> bool {
        match (self.target, other.target) {
            (Some(left), Some(right)) => {
                left == right && self.raw_target.to_lowercase() == other.raw_target.to_lowercase()
            }
            (None, None) => self.raw_target.to_lowercase() == other.raw_target.to_lowercase(),
            _ => false,
        }
    }
}

impl Eq for EmbedKey {}

impl Hash for EmbedKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.target {
            Some(target) => {
                1u8.hash(state);
                target.hash(state);
                self.raw_target.to_lowercase().hash(state);
            }
            None => {
                0u8.hash(state);
                self.raw_target.to_lowercase().hash(state);
            }
        }
    }
}

#[derive(Clone)]
pub(super) enum EmbedState {
    Loading,
    Available(Arc<storage::board::projection::BoardViewProjection>),
    MissingBoard,
    MissingView,
    Ambiguous,
    Error(SharedString),
}

#[derive(Clone)]
struct EmbedBlock {
    key: EmbedKey,
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
        let Node::Paragraph(paragraph) = node else {
            return None;
        };
        let Some(source) = cx.node_source(node) else {
            return None;
        };
        let [Node::Text(text)] = paragraph.children.as_slice() else {
            return None;
        };
        let embeds = storage::board::projection::parse_board_view_embeds(source);
        let [embed] = embeds.as_slice() else {
            return None;
        };
        if text.value.trim() != source.trim() {
            return None;
        }
        let position = node.position()?;
        let key = self
            .states
            .keys()
            .find(|key| key.raw_target.to_lowercase() == embed.raw_target.to_lowercase())
            .cloned()
            .unwrap_or_else(|| EmbedKey::unresolved(embed.raw_target.clone()));
        let block = EmbedBlock {
            key,
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
            EmbedState::Ambiguous => render_status(
                self.editor.clone(),
                block.clone(),
                "This reference matches more than one board or view",
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
    projection: Arc<storage::board::projection::BoardViewProjection>,
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
                                        workspace::WorkspaceNavigationTarget::board(board_id),
                                    ));
                                });
                            }),
                    )
                }),
        )
        .children(
            projection
                .lists
                .iter()
                .filter(|list| !list.cards.is_empty())
                .map(|list| {
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
                                .contains(&storage::board::properties::PropertyKey::DueDate)
                                && let Some(due_on) = card.due_on.as_ref()
                            {
                                metadata.push(due_on.clone());
                            }
                            if projection
                                .visible_properties
                                .contains(&storage::board::properties::PropertyKey::Labels)
                                && !card.labels.is_empty()
                            {
                                metadata.push(card.labels.join(", "));
                            }
                            if projection
                                .visible_properties
                                .contains(&storage::board::properties::PropertyKey::RelatedNotes)
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
                                            if let (Some(board_id), Some(card_id)) =
                                                (board_id, card_id)
                                            {
                                                editor.update(cx, |_, cx| {
                                            cx.emit(super::DocumentEditorEvent::OpenWorkspaceTarget(
                                                workspace::WorkspaceNavigationTarget::card(
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
                }),
        )
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
    let fallback = block.key.raw_target.clone();
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
        let keys = keys_for_content(&content, &self.embeds.states);
        let mut states = self.embeds.states.as_ref().clone();
        states.retain(|key, _| keys.contains(key));
        let keys_to_load = keys
            .iter()
            .cloned()
            .filter(|key| !states.contains_key(key))
            .collect::<HashSet<_>>();
        for key in &keys_to_load {
            states.insert(key.clone(), EmbedState::Loading);
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

    pub fn refresh_board_embeds_for(&mut self, board_id: i64, cx: &mut Context<Self>) {
        let content = self.editor.read(cx).value().to_string();
        let keys = keys_for_content(&content, &self.embeds.states)
            .into_iter()
            .filter(|key| key.target.is_some_and(|target| target.board_id == board_id))
            .collect::<HashSet<_>>();
        if !keys.is_empty() {
            self.start_board_embed_load(keys, cx);
        }
    }

    pub fn reload_board_embeds(&mut self, cx: &mut Context<Self>) {
        let content = self.editor.read(cx).value().to_string();
        let keys = keys_for_content(&content, &self.embeds.states);
        if !keys.is_empty() {
            self.start_board_embed_load(keys, cx);
        }
    }

    fn start_board_embed_load(&mut self, mut keys: HashSet<EmbedKey>, cx: &mut Context<Self>) {
        keys.extend(
            self.embeds
                .loading_keys
                .iter()
                .cloned()
                .filter(|key| self.embeds.states.contains_key(key)),
        );
        self.embeds.loading_keys = keys.clone();
        let generation = self.embeds.request.begin();
        let db = cx.global::<AppRuntime>().store();
        let runtime = cx.global::<AppRuntime>().tokio_handle();
        let task = cx.spawn(async move |this, cx| {
            let (cancel_on_drop, cancelled) = tokio::sync::oneshot::channel::<()>();
            let load = runtime.spawn(async move {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    states = async move {
                        let catalog = storage::workspace::links::load_workspace_reference_catalog(&db)
                            .await?;
                        let mut states = HashMap::new();
                        for key in keys {
                            let resolved = match key.target {
                                Some(target) => Ok(
                                    storage::workspace::links::ResolvedWorkspaceReference::BoardView {
                                        board_id: target.board_id,
                                        view_id: target.view_id,
                                    },
                                ),
                                None => storage::workspace::links::resolve_board_view_target(
                                    &key.raw_target,
                                    &catalog,
                                ),
                            };
                            let (state_key, state) = match resolved {
                                Ok(storage::workspace::links::ResolvedWorkspaceReference::BoardView {
                                    board_id,
                                    view_id,
                                }) => {
                                    let state = match storage::board::projection::load_board_view_projection(
                                        &db,
                                        board_id,
                                        view_id,
                                    )
                                    .await
                                    {
                                        Ok(storage::board::projection::BoardViewProjectionResult::Available(
                                            projection,
                                        )) => EmbedState::Available(Arc::new(projection)),
                                        Ok(storage::board::projection::BoardViewProjectionResult::MissingBoard) => {
                                            EmbedState::MissingBoard
                                        }
                                        Ok(storage::board::projection::BoardViewProjectionResult::MissingView) => {
                                            EmbedState::MissingView
                                        }
                                        Err(error) => EmbedState::Error(error.to_string().into()),
                                    };
                                    (
                                        EmbedKey::resolved(
                                            key.raw_target,
                                            EmbedTarget { board_id, view_id },
                                        ),
                                        state,
                                    )
                                }
                                Err(storage::workspace::links::WorkspaceReferenceResolveError::Missing) => {
                                    (key.clone(), missing_embed_state(&key.raw_target, &catalog))
                                }
                                Err(storage::workspace::links::WorkspaceReferenceResolveError::Ambiguous) => {
                                    (key, EmbedState::Ambiguous)
                                }
                                Err(storage::workspace::links::WorkspaceReferenceResolveError::Invalid) => {
                                    (key, EmbedState::Error("Invalid board reference".into()))
                                }
                                Ok(storage::workspace::links::ResolvedWorkspaceReference::Item(_)) => {
                                    (key, EmbedState::Error("Board view reference expected".into()))
                                }
                            };
                            states.insert(state_key, state);
                        }
                        Ok::<_, anyhow::Error>(states)
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
                    Ok(Some(Ok(states))) => {
                        for (key, state) in states {
                            let matching_raw = merged.keys().any(|existing| {
                                existing.raw_target.to_lowercase() == key.raw_target.to_lowercase()
                            });
                            if !matching_raw {
                                continue;
                            }
                            let stale_keys = merged
                                .keys()
                                .filter(|existing| {
                                    existing.raw_target.to_lowercase()
                                        == key.raw_target.to_lowercase()
                                        && **existing != key
                                })
                                .cloned()
                                .collect::<Vec<_>>();
                            for stale_key in stale_keys {
                                merged.remove(&stale_key);
                            }
                            merged.insert(key, state);
                        }
                    }
                    Ok(Some(Err(error))) => {
                        for key in keys_for_error(&merged, &loading_keys) {
                            merged.insert(key, EmbedState::Error(error.to_string().into()));
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

fn keys_for_content(content: &str, states: &HashMap<EmbedKey, EmbedState>) -> HashSet<EmbedKey> {
    storage::board::projection::parse_board_view_embeds(content)
        .into_iter()
        .map(|embed| {
            states
                .keys()
                .find(|key| key.raw_target.to_lowercase() == embed.raw_target.to_lowercase())
                .cloned()
                .unwrap_or_else(|| EmbedKey::unresolved(embed.raw_target))
        })
        .collect()
}

fn missing_embed_state(
    raw_target: &str,
    catalog: &storage::workspace::links::WorkspaceReferenceCatalog,
) -> EmbedState {
    let Some(reference) = storage::workspace::links::parse_reference_target(raw_target) else {
        return EmbedState::MissingBoard;
    };
    let board_reference = storage::workspace::links::WorkspaceReferencePath {
        kind: reference.kind,
        segments: reference.segments,
        view: None,
        display_text: None,
    };
    match storage::workspace::links::resolve_reference(&board_reference, catalog) {
        Ok(storage::workspace::links::ResolvedWorkspaceReference::Item(_)) => {
            EmbedState::MissingView
        }
        _ => EmbedState::MissingBoard,
    }
}

fn keys_for_error(
    states: &HashMap<EmbedKey, EmbedState>,
    loading: &HashSet<EmbedKey>,
) -> Vec<EmbedKey> {
    if loading.is_empty() {
        states.keys().cloned().collect()
    } else {
        loading.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_embed_keys_keep_distinct_readable_spellings() {
        let target = EmbedTarget {
            board_id: 7,
            view_id: Some(8),
        };
        let current = EmbedKey::resolved("board:Roadmap#Current".into(), target);
        let alias = EmbedKey::resolved("board:Old Roadmap#Now".into(), target);
        assert_ne!(current, alias);

        let mut states = HashMap::new();
        states.insert(current, EmbedState::Loading);
        states.insert(alias, EmbedState::Loading);
        assert_eq!(states.len(), 2);
    }
}
