use super::*;

impl BoardView {
    pub(super) fn render_entry_description(
        &self,
        selected_entry: Option<(&str, &BoardCardDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let has_description =
            matches!(selected_entry, Some((_, entry)) if !entry.description.trim().is_empty());
        let description = match selected_entry {
            Some((_, entry)) if has_description => entry.description.clone(),
            Some(_) => SharedString::from(
                "Add context, acceptance criteria, or links so this card is clear later.",
            ),
            None => SharedString::from("This card is no longer available."),
        };
        let source_project_id = selected_entry.and_then(|(_, entry)| {
            self.related_notes
                .catalog
                .iter()
                .find(|candidate| {
                    candidate.item.kind == storage::workspace_links::WorkspaceItemKind::Card
                        && candidate.item.id == i64::from(entry.id)
                })
                .and_then(|candidate| candidate.project_id)
        });
        let open_target = crate::workspace_navigation::weak_navigation_handler(
            cx.entity().downgrade(),
            |_, target, cx| cx.emit(crate::board::BoardViewEvent::OpenWorkspaceTarget(target)),
        );
        let wikilink_plugin = crate::document_editor::links::WikiLinkPreviewPlugin::new_for_workspace(
            open_target,
            source_project_id,
            self.related_notes.catalog.clone(),
        );
        let placeholder_description = description.clone();

        v_flex()
            .gap_3()
            .p_3()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border.opacity(0.48))
            .bg(theme.secondary.opacity(0.16))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::BookOpen).xsmall())
                            .child("Description"),
                    )
                    .child(
                        Button::new("edit-entry-description")
                            .icon(IconName::Replace)
                            .label("Edit")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_editing_entry(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .min_h(px(72.))
                    .w_full()
                    .text_sm()
                    .line_height(relative(1.5))
                    .whitespace_normal()
                    .text_color(if has_description {
                        theme.popover_foreground
                    } else {
                        theme.muted_foreground
                    })
                    .when_else(
                        has_description,
                        |this| {
                            this.child(
                                TextView::markdown(
                                    "entry-description-markdown",
                                    description.clone(),
                                )
                                .plugin(wikilink_plugin)
                                .style(TextViewStyle::default())
                                .scrollable(false)
                                .selectable(true),
                            )
                        },
                        |this| this.child(placeholder_description),
                    ),
            )
            .child(self.render_entry_attachments(selected_entry, cx))
    }
}
