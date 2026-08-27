use super::*;

impl DocumentEditorView {
    pub(crate) fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let font_size_value = AppSettings::markdown_preview_font_size(cx);
        let font_size = px(font_size_value as f32);

        let sections = if self.analysis.outline.markdown_sections().is_empty() {
            vec![self.editor.read(cx).value()]
        } else {
            self.analysis.outline.markdown_sections().to_vec()
        };
        let section_offsets = if self.analysis.outline.markdown_section_offsets().is_empty() {
            vec![0]
        } else {
            self.analysis.outline.markdown_section_offsets().to_vec()
        };
        let section_count = sections.len();
        let editor_entity = cx.entity();

        let virtualization = markdown_preview_virtualization(self.analysis.outline_rows.is_empty());
        let outline_in_layout = self.analysis.outline_rendered && self.view_width >= px(760.);
        let preview_width = self.view_width
            - if outline_in_layout {
                outline_width_for_view(self.outline_width, self.view_width)
            } else {
                px(0.)
            };

        let horizontal_padding = markdown_preview_horizontal_padding(preview_width);
        let mermaid_width =
            (preview_width.as_f32() - horizontal_padding.as_f32() * 2. - 32.).max(1.);
        let mermaid_snapshots = self.mermaid.render_snapshots(mermaid_width);
        let local_image_plugin = crate::attachments::LocalImagePlugin::new(
            cx.global::<AppRuntime>().data_dir(),
            self.persistence.current_path.as_deref(),
        );
        let open_target =
            workspace::weak_navigation_handler(cx.entity().downgrade(), |_, target, cx| {
                cx.emit(crate::DocumentEditorEvent::OpenWorkspaceTarget(target))
            });
        let wikilink_plugin = crate::links::WikiLinkPreviewPlugin::new(
            open_target,
            self.inspector_links.project_id,
            self.inspector_links.note_catalog.clone(),
            self.inspector_links.note_links.clone(),
            self.inspector_links.workspace_catalog.clone(),
        );
        let board_embed_plugin =
            crate::board_embeds::BoardViewEmbedPlugin::new(cx.entity(), self.embeds.states.clone());
        let preview_style = markdown_preview_style(font_size);

        if self.analysis.preview_list_state.item_count() != section_count {
            self.analysis.preview_list_state.reset(section_count);
        }
        if self
            .analysis
            .preview_font_size_bits
            .replace(font_size_value.to_bits())
            != font_size_value.to_bits()
        {
            self.analysis.preview_list_state.remeasure();
        }

        let content = match virtualization {
            MarkdownPreviewVirtualization::Blocks => TextView::markdown(
                "markdown-preview-blocks",
                sections.into_iter().next().unwrap_or_default(),
            )
            .plugin(local_image_plugin)
            .plugin(board_embed_plugin)
            .plugin(wikilink_plugin)
            .plugin(crate::mermaid::MermaidPlugin::new(
                editor_entity.clone(),
                0,
                mermaid_snapshots,
            ))
            .style(preview_style)
            .code_block_actions(|code_block, _window, _cx| {
                Clipboard::new("copy-code").value(code_block.code().clone())
            })
            .size_full()
            .px(horizontal_padding)
            .py_6()
            .text_size(font_size)
            .scrollable(true)
            .selectable(true)
            .into_any_element(),
            MarkdownPreviewVirtualization::Sections => {
                list(self.analysis.preview_list_state.clone(), {
                    move |index, _window, _cx| {
                        div()
                            .w_full()
                            .px(horizontal_padding)
                            .pt(markdown_preview_section_top_padding(index))
                            .when(index == 0, |this| this.pt_6())
                            .when(index + 1 == section_count, |this| this.pb_6())
                            .child(
                                TextView::markdown(
                                    ("markdown-preview-section", index),
                                    sections[index].clone(),
                                )
                                .plugin(local_image_plugin.clone())
                                .plugin(board_embed_plugin.clone())
                                .plugin(wikilink_plugin.clone())
                                .plugin(crate::mermaid::MermaidPlugin::new(
                                    editor_entity.clone(),
                                    section_offsets.get(index).copied().unwrap_or_default(),
                                    mermaid_snapshots.clone(),
                                ))
                                .style(preview_style.clone())
                                .code_block_actions(|code_block, _window, _cx| {
                                    Clipboard::new("copy-code").value(code_block.code().clone())
                                })
                                .text_size(font_size)
                                .scrollable(false)
                                .selectable(true),
                            )
                            .into_any_element()
                    }
                })
                .size_full()
                .into_any_element()
            }
        };

        div()
            .id("markdown-preview")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(content)
            .when(
                virtualization == MarkdownPreviewVirtualization::Sections,
                |this| this.vertical_scrollbar(&self.analysis.preview_list_state),
            )
    }
}
