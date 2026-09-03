use super::*;
use std::{collections::HashMap, sync::Arc};

impl DocumentEditorView {
    pub(crate) fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (outline_in_layout, _) = editor_layout_signature(
            self.view_width,
            self.analysis.outline_rendered,
            self.outline_width,
        );
        let preview_width = self.view_width
            - if outline_in_layout {
                outline_width_for_view(self.outline_width, self.view_width)
            } else {
                px(0.)
            };

        self.render_preview_with_width(preview_width, cx)
    }

    pub(crate) fn render_preview_with_width(
        &self,
        fallback_width: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let font_size_value = AppSettings::markdown_preview_font_size(cx);
        let font_size = px(font_size_value as f32);

        let sections = if self.analysis.preview_sections.is_empty() {
            let source = self.editor.read(cx).value();
            let base_sections = if self.analysis.outline.markdown_sections().is_empty() {
                vec![source.clone()]
            } else {
                self.analysis.outline.markdown_sections().to_vec()
            };
            Arc::new(prepare_markdown_preview_sections(&source, base_sections))
        } else {
            self.analysis.preview_sections.clone()
        };
        let section_offsets = if self.analysis.outline.markdown_section_offsets().is_empty() {
            vec![0]
        } else {
            self.analysis.outline.markdown_section_offsets().to_vec()
        };
        let section_count = sections.len();
        let editor_entity = cx.entity();

        let virtualization = markdown_preview_virtualization(self.analysis.outline_rows.is_empty());
        let preview_layout_mode = self.mode;
        let preview_width = if self.analysis.preview_bounds_mode == Some(preview_layout_mode) {
            self.analysis
                .preview_bounds
                .map(|bounds| bounds.size.width)
                .unwrap_or(fallback_width)
        } else {
            fallback_width
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
                sections.first().cloned().unwrap_or_default(),
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
            .min_w_0()
            .overflow_hidden()
            .bg(cx.theme().background)
            .on_prepaint({
                let view = cx.entity();
                move |bounds, _, cx| {
                    view.update(cx, |this, cx| {
                        let mode_changed =
                            this.analysis.preview_bounds_mode != Some(preview_layout_mode);
                        if this.analysis.preview_bounds != Some(bounds) || mode_changed {
                            this.analysis.preview_bounds = Some(bounds);
                            this.analysis.preview_bounds_mode = Some(preview_layout_mode);
                            if mode_changed {
                                cx.notify();
                            }
                        }
                    });
                }
            })
            .child(content)
            .when(
                virtualization == MarkdownPreviewVirtualization::Sections,
                |this| this.vertical_scrollbar(&self.analysis.preview_list_state),
            )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreviewFootnoteDefinition {
    identifier: String,
    body: Vec<String>,
}

struct PreviewFootnoteLookup<'a> {
    definitions: &'a [PreviewFootnoteDefinition],
    by_identifier_hash: HashMap<u64, Vec<usize>>,
}

impl<'a> PreviewFootnoteLookup<'a> {
    fn new(definitions: &'a [PreviewFootnoteDefinition]) -> Self {
        let mut by_identifier_hash = HashMap::with_capacity(definitions.len());
        for (index, definition) in definitions.iter().enumerate() {
            by_identifier_hash
                .entry(footnote_identifier_hash(&definition.identifier))
                .or_insert_with(Vec::new)
                .push(index);
        }

        Self {
            definitions,
            by_identifier_hash,
        }
    }

    fn find_index(&self, identifier: &str) -> Option<usize> {
        let candidates = self
            .by_identifier_hash
            .get(&footnote_identifier_hash(identifier))?;
        candidates
            .iter()
            .find(|&&index| {
                self.definitions[index]
                    .identifier
                    .eq_ignore_ascii_case(identifier)
            })
            .copied()
    }

    #[cfg(test)]
    fn find_index_with_work(&self, identifier: &str) -> (Option<usize>, usize) {
        let Some(candidates) = self
            .by_identifier_hash
            .get(&footnote_identifier_hash(identifier))
        else {
            return (None, 0);
        };

        let mut comparisons = 0;
        for &index in candidates {
            comparisons += 1;
            if self.definitions[index]
                .identifier
                .eq_ignore_ascii_case(identifier)
            {
                return (Some(index), comparisons);
            }
        }

        (None, comparisons)
    }
}

fn footnote_identifier_hash(identifier: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    identifier.bytes().fold(FNV_OFFSET, |hash, byte| {
        let byte = if byte.is_ascii_uppercase() {
            byte + (b'a' - b'A')
        } else {
            byte
        };
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

pub(crate) fn prepare_markdown_preview_sections(
    source: &str,
    sections: Vec<SharedString>,
) -> Vec<SharedString> {
    let definitions = collect_footnote_definitions(source);
    if definitions.is_empty() {
        return sections;
    }

    let lookup = PreviewFootnoteLookup::new(&definitions);
    let mut rendered_sections = sections
        .iter()
        .map(|section| render_footnote_section(section, &lookup))
        .collect::<Vec<_>>();
    if let Some(last) = rendered_sections.last_mut() {
        let footer = render_footnote_footer(&lookup);
        if !last.trim().is_empty() {
            while last.ends_with('\n') {
                last.pop();
            }
            last.push_str("\n\n");
        }
        last.push_str(&footer);
    }

    rendered_sections
        .into_iter()
        .map(SharedString::from)
        .collect()
}

fn collect_footnote_definitions(source: &str) -> Vec<PreviewFootnoteDefinition> {
    let mut definitions = Vec::new();
    let mut active_definition = None;
    let mut fence = None;

    for line in source.lines() {
        if let Some((character, length)) = markdown_fence(line) {
            if fence.is_some_and(|(open_character, open_length)| {
                open_character == character && length >= open_length
            }) {
                fence = None;
            } else if fence.is_none() {
                fence = Some((character, length));
            }
            active_definition = None;
            continue;
        }

        if fence.is_some() {
            continue;
        }

        if let Some((identifier, body)) = parse_footnote_definition(line) {
            definitions.push(PreviewFootnoteDefinition {
                identifier,
                body: vec![body],
            });
            active_definition = Some(definitions.len() - 1);
        } else if let Some(index) = active_definition {
            if let Some(body) = footnote_continuation(line) {
                definitions[index].body.push(body);
            } else if !line.trim().is_empty() {
                active_definition = None;
            }
        }
    }

    definitions
}

fn render_footnote_section(section: &str, lookup: &PreviewFootnoteLookup<'_>) -> String {
    let mut rendered = Vec::new();
    let mut active_definition = false;
    let mut fence = None;

    for line in section.lines() {
        if let Some((character, length)) = markdown_fence(line) {
            if fence.is_some_and(|(open_character, open_length)| {
                open_character == character && length >= open_length
            }) {
                fence = None;
            } else if fence.is_none() {
                fence = Some((character, length));
            }
            active_definition = false;
            rendered.push(line.to_string());
            continue;
        }

        if fence.is_some() {
            rendered.push(line.to_string());
            continue;
        }

        if parse_footnote_definition(line).is_some() {
            active_definition = true;
            continue;
        }
        if active_definition && footnote_continuation(line).is_some() {
            continue;
        }
        if !line.trim().is_empty() {
            active_definition = false;
        }

        rendered.push(replace_footnote_references(line, lookup));
    }

    rendered.join("\n")
}

fn render_footnote_footer(lookup: &PreviewFootnoteLookup<'_>) -> String {
    let mut footer = String::from("#### Footnotes");

    for (index, definition) in lookup.definitions.iter().enumerate() {
        footer.push_str("\n\n");
        let first_line = definition.body.first().map(String::as_str).unwrap_or("");
        footer.push_str(&format!(
            "{}. {}",
            index + 1,
            replace_footnote_references(first_line, lookup)
        ));
        for line in definition.body.iter().skip(1) {
            footer.push_str("\n   ");
            footer.push_str(&replace_footnote_references(line, lookup));
        }
    }

    footer
}

fn parse_footnote_definition(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 || !trimmed.starts_with("[^") {
        return None;
    }

    let close = trimmed.find("]:")?;
    let identifier = &trimmed[2..close];
    if identifier.is_empty()
        || identifier
            .chars()
            .any(|character| character.is_whitespace() || character == '[' || character == ']')
    {
        return None;
    }

    let body = trimmed[close + 2..]
        .strip_prefix(' ')
        .or_else(|| trimmed[close + 2..].strip_prefix('\t'))
        .unwrap_or(&trimmed[close + 2..])
        .to_string();
    Some((identifier.to_string(), body))
}

fn footnote_continuation(line: &str) -> Option<String> {
    if let Some(body) = line.strip_prefix('\t') {
        return Some(body.to_string());
    }
    line.strip_prefix("    ").map(str::to_string)
}

fn markdown_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }

    let character = trimmed.chars().next()?;
    if character != '`' && character != '~' {
        return None;
    }

    let length = trimmed
        .chars()
        .take_while(|item| *item == character)
        .count();
    (length >= 3).then_some((character, length))
}

fn replace_footnote_references(line: &str, lookup: &PreviewFootnoteLookup<'_>) -> String {
    let mut rendered = String::with_capacity(line.len());
    let mut index = 0;
    let mut code_ticks = 0;

    while index < line.len() {
        if line.as_bytes()[index] == b'`' {
            let length = line[index..]
                .bytes()
                .take_while(|character| *character == b'`')
                .count();
            rendered.push_str(&line[index..index + length]);
            if code_ticks == 0 {
                code_ticks = length;
            } else if code_ticks == length {
                code_ticks = 0;
            }
            index += length;
            continue;
        }

        if code_ticks == 0
            && line[index..].starts_with("[^")
            && let Some(close) = line[index + 2..].find(']')
        {
            let end = index + 2 + close;
            let identifier = &line[index + 2..end];
            if let Some(footnote_index) = lookup.find_index(identifier) {
                rendered.push_str(&footnote_marker(footnote_index + 1));
                index = end + 1;
                continue;
            }
        }

        let character = line[index..]
            .chars()
            .next()
            .expect("index must remain on a character boundary");
        rendered.push(character);
        index += character.len_utf8();
    }

    rendered
}

fn footnote_marker(index: usize) -> String {
    const SUPERSCRIPT_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    index
        .to_string()
        .chars()
        .map(|character| {
            character
                .to_digit(10)
                .and_then(|digit| SUPERSCRIPT_DIGITS.get(digit as usize).copied())
                .unwrap_or(character)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use gpui::SharedString;

    use super::{
        PreviewFootnoteLookup, collect_footnote_definitions, prepare_markdown_preview_sections,
    };

    #[test]
    fn renders_footnotes_as_markers_and_a_footer() {
        let source = "A claim[^source].\n\n[^source]: Read the primary source.";
        let sections =
            prepare_markdown_preview_sections(source, vec![SharedString::from(source.to_string())]);

        assert_eq!(
            sections,
            vec![SharedString::from(
                "A claim¹.\n\n#### Footnotes\n\n1. Read the primary source.".to_string()
            )]
        );
    }

    #[test]
    fn preserves_code_and_inline_code_that_look_like_footnotes() {
        let source =
            "`[^inline]`\n\n```markdown\n[^fenced]: not a note\n```\n\n[^real]: Actual note";
        let sections =
            prepare_markdown_preview_sections(source, vec![SharedString::from(source.to_string())]);
        let rendered = sections[0].to_string();

        assert!(rendered.contains("`[^inline]`"));
        assert!(rendered.contains("[^fenced]: not a note"));
        assert!(rendered.contains("Actual note"));
        assert!(!rendered.contains("[^real]:"));
        assert!(rendered.contains("#### Footnotes"));
    }

    #[test]
    fn preserves_first_definition_for_case_insensitive_duplicate_identifiers() {
        let source =
            "Claim[^Source].\n\n[^source]: First definition.\n[^SOURCE]: Second definition.";
        let sections = prepare_markdown_preview_sections(source, vec![SharedString::from(source)]);

        assert!(sections[0].starts_with("Claim¹."));
        assert!(sections[0].contains("1. First definition."));
        assert!(sections[0].contains("2. Second definition."));
    }

    #[test]
    fn removes_definitions_from_their_sections_and_appends_one_footer() {
        let source = "# Intro\nA claim[^1].\n\n# Sources\n[^1]: Primary source.";
        let sections = vec![
            SharedString::from("# Intro\nA claim[^1].".to_string()),
            SharedString::from("# Sources\n[^1]: Primary source.".to_string()),
        ];
        let rendered = prepare_markdown_preview_sections(source, sections);

        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0], "# Intro\nA claim¹.");
        assert_eq!(
            rendered[1],
            "# Sources\n\n#### Footnotes\n\n1. Primary source."
        );
    }

    #[test]
    fn large_footnote_preview_fixture_proves_indexed_definition_lookup_work() {
        const DEFINITION_COUNT: usize = 2_048;
        const REFERENCE_COUNT: usize = 2_048;

        let references = (0..REFERENCE_COUNT)
            .map(|index| format!("Claim [{0}][^note{0}].\n", index % DEFINITION_COUNT))
            .collect::<String>();
        let definitions = (0..DEFINITION_COUNT)
            .map(|index| format!("[^note{index}]: Definition {index}.\n"))
            .collect::<String>();
        let source = format!("{references}\n{definitions}");
        let started = std::time::Instant::now();
        let rendered =
            prepare_markdown_preview_sections(&source, vec![SharedString::from(source.as_str())]);
        let elapsed = started.elapsed();
        let definitions = collect_footnote_definitions(&source);
        let lookup = PreviewFootnoteLookup::new(&definitions);
        let mut linear_lookup_comparisons = 0;
        let mut indexed_lookup_comparisons = 0;
        for index in 0..REFERENCE_COUNT {
            let expected_index = index % DEFINITION_COUNT;
            let identifier = format!("note{expected_index}");
            let mut linear_index = None;
            let mut linear_comparisons = 0;
            for (definition_index, definition) in definitions.iter().enumerate() {
                linear_comparisons += 1;
                if definition.identifier.eq_ignore_ascii_case(&identifier) {
                    linear_index = Some(definition_index);
                    break;
                }
            }
            linear_lookup_comparisons += linear_comparisons;

            let (actual_index, comparisons) = lookup.find_index_with_work(&identifier);
            assert_eq!(linear_index, actual_index);
            assert_eq!(linear_index, Some(expected_index));
            assert_eq!(actual_index, Some(expected_index));
            indexed_lookup_comparisons += comparisons;
        }

        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].contains("#### Footnotes"));
        assert!(linear_lookup_comparisons > 2_000_000);
        assert_eq!(indexed_lookup_comparisons, REFERENCE_COUNT);
        println!(
            "markdown_definitions={DEFINITION_COUNT} markdown_references={REFERENCE_COUNT} linear_body_lookup_comparisons={linear_lookup_comparisons} indexed_lookup_comparisons={indexed_lookup_comparisons} elapsed_micros={}",
            elapsed.as_micros(),
        );
    }
}
