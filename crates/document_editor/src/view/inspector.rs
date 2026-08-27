use super::*;

impl DocumentEditorView {
    pub(super) fn render_inspector_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_1()
            .child(
                Button::new("document-inspector-outline")
                    .label("Outline")
                    .ghost()
                    .small()
                    .selected(self.inspector_links.tab == DocumentInspectorTab::Outline)
                    .on_click(cx.listener(|this, _, _, cx| this.show_outline_inspector(cx))),
            )
            .children(
                (self.kind == DocumentKind::Json && self.analysis.outline.json_has_error()).then(
                    || {
                        Icon::new(IconName::TriangleAlert)
                            .xsmall()
                            .text_color(cx.theme().warning)
                    },
                ),
            )
            .children((self.kind == DocumentKind::Markdown).then(|| {
                Button::new("document-inspector-links")
                    .label("Links")
                    .ghost()
                    .small()
                    .selected(self.inspector_links.tab == DocumentInspectorTab::Links)
                    .on_click(cx.listener(|this, _, _, cx| this.show_links_inspector(cx)))
            }))
    }

    pub(super) fn render_links_inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let inbound = self.inspector_links.note_links.inbound.clone();
        let outbound = self.inspector_links.note_links.outbound.clone();
        let inbound_rows = if inbound.is_empty() {
            vec![link_empty_state("No notes link here yet", cx).into_any_element()]
        } else {
            inbound
                .iter()
                .enumerate()
                .map(|(index, link)| {
                    self.render_note_link_row("inbound-note-link", index, link, true, cx)
                })
                .collect()
        };
        let outbound_rows = if outbound.is_empty() {
            vec![link_empty_state("This note has no links", cx).into_any_element()]
        } else {
            outbound
                .iter()
                .enumerate()
                .map(
                    |(index, link)| match (link.target_note_id, link.target_title.as_deref()) {
                        (Some(_), Some(_)) => {
                            self.render_note_link_row("outbound-note-link", index, link, false, cx)
                        }
                        _ => h_flex()
                            .id(("unresolved-note-link", index))
                            .min_h_9()
                            .px_3()
                            .gap_2()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(Icon::new(IconName::TriangleAlert).xsmall())
                            .child(div().min_w_0().truncate().child(link.raw_target.clone()))
                            .into_any_element(),
                    },
                )
                .collect()
        };
        let board_rows = self
            .render_workspace_link_rows(storage::workspace::links::WorkspaceItemKind::Board, cx);
        let list_rows =
            self.render_workspace_link_rows(storage::workspace::links::WorkspaceItemKind::List, cx);
        let card_rows =
            self.render_workspace_link_rows(storage::workspace::links::WorkspaceItemKind::Card, cx);

        v_flex()
            .id("document-links")
            .relative()
            .w(outline_width)
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar.opacity(0.72))
            .child(
                h_flex()
                    .h_10()
                    .px_3()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.7))
                    .child(self.render_inspector_tabs(cx))
                    .child(
                        Button::new("close-document-links")
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .tooltip("Hide inspector (Ctrl+Shift+O)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_outline(window, cx);
                            })),
                    ),
            )
            .when(self.inspector_links.loading, |this| {
                this.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading links…"),
                )
            })
            .when_some(self.inspector_links.error.clone(), |this, error| {
                this.child(
                    v_flex()
                        .p_4()
                        .gap_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child("Could not load note links")
                        .child(div().text_xs().child(error)),
                )
            })
            .when(
                !self.inspector_links.loading && self.inspector_links.error.is_none(),
                |this| {
                    this.child(
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .child(link_section_title("Links to this note", cx))
                            .children(inbound_rows)
                            .child(link_section_title("Links from this note", cx))
                            .children(outbound_rows)
                            .child(link_section_title("Board references", cx))
                            .when(
                                board_rows.is_empty()
                                    && list_rows.is_empty()
                                    && card_rows.is_empty(),
                                |this| {
                                    this.child(link_empty_state(
                                        "This note has no board references",
                                        cx,
                                    ))
                                },
                            )
                            .when(!board_rows.is_empty(), |this| {
                                this.child(link_group_title("Boards", cx))
                                    .children(board_rows)
                            })
                            .when(!list_rows.is_empty(), |this| {
                                this.child(link_group_title("Lists", cx))
                                    .children(list_rows)
                            })
                            .when(!card_rows.is_empty(), |this| {
                                this.child(link_group_title("Cards", cx))
                                    .children(card_rows)
                            }),
                    )
                },
            )
    }

    fn render_workspace_link_rows(
        &self,
        kind: storage::workspace::links::WorkspaceItemKind,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut seen = HashSet::new();
        self.inspector_links
            .workspace_links
            .references
            .iter()
            .filter(|reference| reference.item.item.kind == kind)
            .filter(|reference| seen.insert(reference.item.item))
            .filter_map(|reference| {
                let target = crate::links::workspace_navigation_target(&reference.item)?;
                let item_id = reference.item.item.id;
                let label = reference.item.breadcrumb();
                let origin = match reference.origin {
                    storage::workspace::links::WorkspaceLinkOrigin::Manual => "Linked",
                    storage::workspace::links::WorkspaceLinkOrigin::Wikilink => "Markdown",
                    storage::workspace::links::WorkspaceLinkOrigin::Embed => "Embed",
                };
                Some(
                    h_flex()
                        .id(("workspace-link-reference", item_id as u64))
                        .min_h_9()
                        .px_3()
                        .py_1()
                        .cursor_pointer()
                        .hover(|this| this.bg(cx.theme().accent.opacity(0.38)))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(crate::DocumentEditorEvent::OpenWorkspaceTarget(target));
                        }))
                        .child(
                            v_flex()
                                .min_w_0()
                                .flex_1()
                                .child(div().text_sm().truncate().child(label))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground.opacity(0.72))
                                        .child(origin),
                                ),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }

    fn render_note_link_row(
        &self,
        id: &'static str,
        index: usize,
        link: &storage::note::links::NoteLinkReference,
        inbound: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (note_id, title, project_name, source_offset) = if inbound {
            (
                link.source_note_id as u32,
                link.source_title.as_str(),
                link.source_project_name.as_deref(),
                Some(link.start_byte),
            )
        } else {
            (
                link.target_note_id.unwrap_or_default() as u32,
                link.target_title.as_deref().unwrap_or(&link.raw_target),
                link.target_project_name.as_deref(),
                None,
            )
        };

        h_flex()
            .id((id, index))
            .min_h_10()
            .px_3()
            .py_1()
            .gap_2()
            .cursor_pointer()
            .hover(|this| this.bg(cx.theme().accent.opacity(0.38)))
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(crate::DocumentEditorEvent::OpenNote {
                    note_id,
                    source_offset,
                });
            }))
            .child(Icon::new(IconName::File).xsmall())
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(title.to_string()))
                    .children(project_name.map(|project| {
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(project.to_string())
                    })),
            )
            .into_any_element()
    }
}
