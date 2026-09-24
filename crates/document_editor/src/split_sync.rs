use super::document_state::EditorMode;
use super::outline::DocumentOutline;
use super::{DocumentEditorView, DocumentKind};
use gpui_kit::component::input::RopeExt as _;
use gpui_kit::{App, Context, ListOffset, WeakEntity, point, px};
use std::time::Duration;

const SPLIT_SYNC_ATTEMPTS: usize = 4;
const SPLIT_SYNC_DELAY: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(super) struct SplitSyncState {
    pub(super) last_source_top_row: Option<usize>,
}

pub(super) fn preview_section_for_source_line(
    line: usize,
    outline: &DocumentOutline,
) -> Option<usize> {
    if !matches!(outline, DocumentOutline::Markdown(_)) {
        return None;
    }
    let rows = outline.rows();
    if rows.is_empty() {
        return None;
    }
    if let Some(active) = outline.active_markdown_index_for_line(line) {
        return rows.get(active)?.preview_section_index;
    }
    Some(0)
}

pub(super) fn source_line_for_preview_section(
    section: usize,
    outline: &DocumentOutline,
) -> Option<usize> {
    if !matches!(outline, DocumentOutline::Markdown(_)) {
        return None;
    }
    let rows = outline.rows();
    if rows.is_empty() {
        return None;
    }
    if let Some(row) = rows
        .iter()
        .find(|row| row.preview_section_index == Some(section))
    {
        return Some(row.source_line);
    }
    if section == 0 {
        return Some(0);
    }
    None
}

pub(super) fn pixel_target_for_fraction(fraction: f32, max: f32) -> f32 {
    fraction.clamp(0.0, 1.0) * max.max(0.0)
}

pub(super) fn pixel_offset_for_line(line: usize, line_height: f32) -> f32 {
    (line as f32 * line_height.max(0.0)).max(0.0)
}

pub(super) fn should_scroll_pixels(current: f32, target: f32) -> bool {
    (target - current).abs() > 1.0
}

impl DocumentEditorView {
    pub(super) fn sync_preview_from_source(&mut self, cx: &mut Context<Self>) -> bool {
        if self.mode != EditorMode::Split || self.kind != DocumentKind::Markdown {
            return true;
        }
        let editor = self.editor.read(cx);
        let Some(line_height) = editor.line_height() else {
            return false;
        };
        let top_pixel = -editor.scroll_offset().y.as_f32();
        let top = (top_pixel / line_height.as_f32().max(1.0)).floor().max(0.0) as usize;
        if self.split_sync.last_source_top_row == Some(top) {
            return true;
        }
        self.split_sync.last_source_top_row = Some(top);

        if self.analysis.outline_rows.is_empty() {
            return self.sync_blocks_preview_from_source_pixel(cx);
        }
        let Some(target) = preview_section_for_source_line(top, &self.analysis.outline) else {
            return true;
        };
        let current = self
            .analysis
            .preview_list_state
            .logical_scroll_top()
            .item_ix;
        if current == target {
            return true;
        }
        self.analysis.preview_list_state.scroll_to(ListOffset {
            item_ix: target,
            offset_in_item: px(0.),
        });
        cx.notify();
        true
    }

    fn sync_blocks_preview_from_source_pixel(&mut self, cx: &mut Context<Self>) -> bool {
        let editor = self.editor.read(cx);
        let Some(line_height) = editor.line_height() else {
            return false;
        };
        let Some(source_bounds) = self.analysis.source_bounds else {
            return false;
        };
        let total_lines = editor.text().lines_len();
        let current_source = -editor.scroll_offset().y.as_f32();
        let source_max =
            total_lines as f32 * line_height.as_f32() - source_bounds.size.height.as_f32();
        if source_max <= 0.0 {
            return true;
        }
        let fraction = (current_source / source_max).clamp(0.0, 1.0);
        let preview_max = self
            .preview_blocks_list
            .max_offset_for_scrollbar()
            .y
            .as_f32();
        if preview_max <= 0.0 {
            return false;
        }
        let preview_current = -self
            .preview_blocks_list
            .scroll_px_offset_for_scrollbar()
            .y
            .as_f32();
        let target = pixel_target_for_fraction(fraction, preview_max);
        if !should_scroll_pixels(preview_current, target) {
            return true;
        }
        self.preview_blocks_list
            .scroll_by(px(target - preview_current));
        cx.notify();
        true
    }

    pub(super) fn request_split_sync(&mut self, cx: &mut Context<Self>) {
        if self.mode != EditorMode::Split || self.kind != DocumentKind::Markdown {
            return;
        }
        self.split_sync.last_source_top_row = None;
        cx.spawn(async move |this, cx| {
            for _ in 0..SPLIT_SYNC_ATTEMPTS {
                cx.background_executor().timer(SPLIT_SYNC_DELAY).await;
                let synced = this
                    .update(cx, |this, cx| {
                        if this.mode != EditorMode::Split {
                            return true;
                        }
                        this.sync_preview_from_source(cx)
                    })
                    .unwrap_or(true);
                if synced {
                    return;
                }
            }
        })
        .detach();
    }

    pub(super) fn sync_source_from_preview_section(
        section: usize,
        view: WeakEntity<Self>,
        cx: &mut App,
    ) {
        let Some((editor, source_viewport, target_line)) = view
            .read_with(cx, |this, _| {
                if this.mode != EditorMode::Split || this.kind != DocumentKind::Markdown {
                    return None;
                }
                let line = source_line_for_preview_section(section, &this.analysis.outline)?;
                let viewport = this.analysis.source_bounds?.size.height.as_f32();
                Some((this.editor.clone(), viewport, line))
            })
            .ok()
            .flatten()
        else {
            return;
        };
        editor.update(cx, |editor, cx| {
            let Some(line_height) = editor.line_height() else {
                return;
            };
            let total_lines = editor.text().lines_len();
            let source_max = total_lines as f32 * line_height.as_f32() - source_viewport;
            if source_max <= 0.0 {
                return;
            }
            let target =
                pixel_offset_for_line(target_line, line_height.as_f32()).clamp(0.0, source_max);
            let current = editor.scroll_offset();
            let current_pixel = -current.y.as_f32();
            if !should_scroll_pixels(current_pixel, target) {
                return;
            }
            editor.set_scroll_offset(point(current.x, px(-target)), cx);
        });
    }

    pub(super) fn sync_source_from_preview_fraction(
        fraction: f32,
        view: WeakEntity<Self>,
        cx: &mut App,
    ) {
        let fraction = fraction.clamp(0.0, 1.0);
        let Some((editor, source_viewport)) = view
            .read_with(cx, |this, _| {
                if this.mode != EditorMode::Split || this.kind != DocumentKind::Markdown {
                    return None;
                }
                let viewport = this.analysis.source_bounds?.size.height.as_f32();
                Some((this.editor.clone(), viewport))
            })
            .ok()
            .flatten()
        else {
            return;
        };
        editor.update(cx, |editor, cx| {
            let Some(line_height) = editor.line_height() else {
                return;
            };
            let total_lines = editor.text().lines_len();
            let source_max = total_lines as f32 * line_height.as_f32() - source_viewport;
            if source_max <= 0.0 {
                return;
            }
            let target = pixel_target_for_fraction(fraction, source_max);
            let current = editor.scroll_offset();
            let current_pixel = -current.y.as_f32();
            if !should_scroll_pixels(current_pixel, target) {
                return;
            }
            editor.set_scroll_offset(point(current.x, px(-target)), cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        pixel_offset_for_line, pixel_target_for_fraction, preview_section_for_source_line,
        should_scroll_pixels, source_line_for_preview_section,
    };
    use crate::outline::MarkdownOutline;

    fn outline_for(source: &str) -> crate::outline::DocumentOutline {
        crate::outline::DocumentOutline::Markdown(MarkdownOutline::parse(source))
    }

    #[test]
    fn source_line_maps_to_its_heading_preview_section() {
        let outline = outline_for("# One\nBody\n## Two\nMore\n");
        let rows = outline.rows();

        assert_eq!(rows.len(), 2);
        assert_eq!(
            preview_section_for_source_line(0, &outline),
            rows[0].preview_section_index
        );
        assert_eq!(
            preview_section_for_source_line(2, &outline),
            rows[1].preview_section_index
        );
        assert_eq!(
            preview_section_for_source_line(100, &outline),
            rows[1].preview_section_index
        );
    }

    #[test]
    fn lines_before_the_first_heading_map_to_the_preamble_section() {
        let outline = outline_for("Intro\n\n# One\nBody\n");

        assert_eq!(preview_section_for_source_line(0, &outline), Some(0));
        assert_eq!(preview_section_for_source_line(1, &outline), Some(0));
        assert_eq!(
            preview_section_for_source_line(2, &outline),
            outline.rows()[0].preview_section_index
        );
    }

    #[test]
    fn source_mapping_returns_none_without_markdown_headings() {
        let outline = outline_for("Plain text without headings\n");

        assert_eq!(preview_section_for_source_line(0, &outline), None);
        assert_eq!(preview_section_for_source_line(5, &outline), None);
    }

    #[test]
    fn preview_section_maps_back_to_its_heading_source_line() {
        let outline = outline_for("Intro\n\n# One\nBody\n## Two\nMore\n");
        let rows = outline.rows();

        assert_eq!(
            source_line_for_preview_section(0, &outline),
            Some(0),
            "preamble section starts at the document start"
        );
        for row in &rows {
            let section = row
                .preview_section_index
                .expect("markdown rows should map to a preview section");
            assert_eq!(
                source_line_for_preview_section(section, &outline),
                Some(row.source_line)
            );
        }
    }

    #[test]
    fn preview_mapping_returns_none_for_unknown_sections() {
        let outline = outline_for("# One\nBody\n");

        assert_eq!(source_line_for_preview_section(999, &outline), None);
    }

    #[test]
    fn preview_mapping_returns_none_without_markdown_headings() {
        let outline = outline_for("Plain text\n");

        assert_eq!(source_line_for_preview_section(0, &outline), None);
    }

    #[test]
    fn pixel_target_scales_with_fraction_and_clamps() {
        assert_eq!(pixel_target_for_fraction(0.0, 400.0), 0.0);
        assert_eq!(pixel_target_for_fraction(0.5, 400.0), 200.0);
        assert_eq!(pixel_target_for_fraction(1.0, 400.0), 400.0);
        assert_eq!(pixel_target_for_fraction(2.0, 400.0), 400.0);
        assert_eq!(pixel_target_for_fraction(-1.0, 400.0), 0.0);
        assert_eq!(pixel_target_for_fraction(0.5, -10.0), 0.0);
    }

    #[test]
    fn pixel_offset_for_line_scales_with_line_height() {
        assert_eq!(pixel_offset_for_line(0, 20.0), 0.0);
        assert_eq!(pixel_offset_for_line(3, 20.0), 60.0);
        assert_eq!(pixel_offset_for_line(3, -5.0), 0.0);
    }

    #[test]
    fn pixel_sync_ignores_subpixel_jitter() {
        assert!(!should_scroll_pixels(100.0, 100.5));
        assert!(!should_scroll_pixels(100.0, 101.0));
        assert!(should_scroll_pixels(100.0, 102.0));
        assert!(should_scroll_pixels(0.0, 400.0));
    }
}
