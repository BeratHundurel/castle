use super::document_state::EditorMode;
use super::outline::DocumentOutline;
use super::{DocumentEditorView, DocumentKind};
use gpui_kit::component::input::{EditorState, RopeExt as _};
use gpui_kit::{
    App, Bounds, Context, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, IntoElement, LayoutId, ListOffset, ListState, Pixels, Position,
    ScrollWheelEvent, Style, Task, WeakEntity, Window, point, px, relative,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const SPLIT_SYNC_ATTEMPTS: usize = 4;
const SPLIT_SYNC_DELAY: Duration = Duration::from_millis(16);
const PREVIEW_SCROLL_SETTLE_ATTEMPTS: usize = 12;
const PREVIEW_SCROLL_IDLE_DELAY: Duration = Duration::from_millis(120);

#[derive(Clone, Copy)]
enum PreviewSourceTarget {
    Line(f32),
    BottomVisible,
    NoCorrection,
}

#[derive(Default)]
pub(super) struct SplitSyncState {
    pub(super) last_source_top_pixel: Option<f32>,
    last_source_line_position: Option<f32>,
    last_preview_scroll_top: Option<ListOffset>,
    pub(super) preview_wheel_direction: Rc<Cell<Option<f32>>>,
    pub(super) preview_drives_source: bool,
    pending_preview_scroll_settle: Option<Task<()>>,
}

pub(super) struct PreviewScrollBoundaryGuard {
    list: ListState,
    wheel_direction: Rc<Cell<Option<f32>>>,
    view: WeakEntity<DocumentEditorView>,
}

impl PreviewScrollBoundaryGuard {
    pub(super) fn new(
        list: ListState,
        wheel_direction: Rc<Cell<Option<f32>>>,
        view: WeakEntity<DocumentEditorView>,
    ) -> Self {
        Self {
            list,
            wheel_direction,
            view,
        }
    }
}

impl IntoElement for PreviewScrollBoundaryGuard {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PreviewScrollBoundaryGuard {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style {
            position: Position::Absolute,
            ..Default::default()
        };
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        let viewport_bounds = Bounds {
            origin: point(bounds.origin.x, bounds.origin.y - bounds.size.height),
            size: bounds.size,
        };
        window.insert_hitbox(viewport_bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        let hitbox_id = hitbox.id;
        let list = self.list.clone();
        let wheel_direction = self.wheel_direction.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture || !hitbox_id.should_handle_scroll(window) {
                return;
            }
            let delta = event.delta.pixel_delta(window.line_height());
            if delta.y.abs() <= delta.x.abs() {
                return;
            }
            wheel_direction.set(Some(-delta.y.as_f32().signum()));
            let current = -list.scroll_px_offset_for_scrollbar().y;
            let max = list.max_offset_for_scrollbar().y;
            if delta.y < px(0.) && max - current <= px(1.) {
                let source_needs_scroll = view
                    .read_with(cx, |this, cx| {
                        if this.mode != EditorMode::Split || this.kind != DocumentKind::Markdown {
                            return false;
                        }
                        let Some(source_bounds) = this.analysis.source_bounds else {
                            return false;
                        };
                        let editor = this.editor.read(cx);
                        source_scroll_target_for_bottom_visible(editor, source_bounds)
                            .is_some_and(|target| target + editor.scroll_offset().y.as_f32() > 1.0)
                    })
                    .unwrap_or(false);
                if source_needs_scroll {
                    return;
                }
            }
            if (delta.y > px(0.) && current <= px(1.))
                || (delta.y < px(0.) && max - current <= px(1.))
            {
                cx.stop_propagation();
            }
        });
    }
}

pub(super) fn preview_section_for_source_line(
    line: usize,
    outline: &DocumentOutline,
) -> Option<usize> {
    if !matches!(outline, DocumentOutline::Markdown(_)) {
        return None;
    }
    let rows = outline.markdown_rows();
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
    let rows = outline.markdown_rows();
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

fn preview_scroll_direction(
    previous: ListOffset,
    current: ListOffset,
    wheel_direction: Option<f32>,
) -> f32 {
    let logical_direction = match current.item_ix.cmp(&previous.item_ix) {
        std::cmp::Ordering::Less => -1.0,
        std::cmp::Ordering::Equal => (current.offset_in_item - previous.offset_in_item)
            .as_f32()
            .signum(),
        std::cmp::Ordering::Greater => 1.0,
    };
    if logical_direction == 0.0 {
        0.0
    } else {
        wheel_direction.unwrap_or(logical_direction).signum()
    }
}

fn section_source_line_span(
    section: usize,
    outline: &DocumentOutline,
    total_lines: usize,
) -> Option<(f32, f32)> {
    let start = source_line_for_preview_section(section, outline)?;
    let end = source_line_for_preview_section(section + 1, outline).unwrap_or(total_lines);
    Some((start as f32, end.max(start + 1) as f32))
}

fn source_top_line_position(editor: &EditorState, source_bounds: Bounds<Pixels>) -> Option<f32> {
    let line = editor.visible_row_range()?.start;
    let start = editor.text().line_start_offset(line);
    let end = editor.text().line_end_offset(line);
    let line_bounds = editor.range_to_bounds(&(start..end))?;
    let progress =
        ((source_bounds.top() - line_bounds.top()) / line_bounds.size.height).clamp(0.0, 1.0);
    Some(line as f32 + progress)
}

fn source_scroll_target_for_line_position(
    editor: &EditorState,
    source_bounds: Bounds<Pixels>,
    target_line_position: f32,
) -> Option<f32> {
    let current_pixel = -editor.scroll_offset().y.as_f32();
    let line = target_line_position.floor() as usize;
    let start = editor.text().line_start_offset(line);
    let end = editor.text().line_end_offset(line);
    if editor.visible_row_range()?.contains(&line)
        && let Some(bounds) = editor.range_to_bounds(&(start..end))
    {
        let fraction = target_line_position.fract();
        return Some(
            (current_pixel
                + (bounds.top() - source_bounds.top()).as_f32()
                + fraction * bounds.size.height.as_f32())
            .max(0.0),
        );
    }
    let current_line_position = source_top_line_position(editor, source_bounds)?;
    let line_height = editor.line_height()?.as_f32();
    Some((current_pixel + (target_line_position - current_line_position) * line_height).max(0.0))
}

fn source_scroll_target_for_bottom_visible(
    editor: &EditorState,
    source_bounds: Bounds<Pixels>,
) -> Option<f32> {
    let current_pixel = -editor.scroll_offset().y.as_f32();
    let line_height = editor.line_height()?.as_f32();
    let max_step = (source_bounds.size.height.as_f32() * 0.25).max(line_height);
    let last_line = editor.text().lines_len().saturating_sub(1);
    let start = editor.text().line_start_offset(last_line);
    let end = editor.text().line_end_offset(last_line);
    let target_pixel = editor
        .range_to_bounds(&(start..end))
        .map(|bounds| current_pixel + (bounds.bottom() - source_bounds.bottom()).as_f32())
        // The final line is outside the laid-out range; advance and measure again.
        .unwrap_or(current_pixel + max_step);
    // Cap measured corrections too, so a long wrapped line cannot produce a
    // large one-frame jump when it first enters the laid-out range.
    Some(target_pixel.clamp(current_pixel, current_pixel + max_step))
}

fn scroll_target_from_samples(
    previous: (f32, f32),
    current: (f32, f32),
    target_line_position: f32,
) -> Option<f32> {
    let (previous_pixel, previous_line) = previous;
    let (current_pixel, current_line) = current;
    let lines_per_pixel = (current_line - previous_line) / (current_pixel - previous_pixel);
    (lines_per_pixel.is_finite() && lines_per_pixel > 0.0)
        .then(|| (current_pixel + (target_line_position - current_line) / lines_per_pixel).max(0.0))
}

fn preview_offset_for_source_position(
    source: f32,
    source_start: f32,
    source_end: f32,
    preview_height: f32,
) -> f32 {
    ((source - source_start) / (source_end - source_start).max(1.0)).clamp(0.0, 1.0)
        * preview_height.max(0.0)
}

fn source_position_for_preview_offset(
    preview: f32,
    preview_height: f32,
    source_start: f32,
    source_end: f32,
) -> f32 {
    source_start
        + (preview / preview_height.max(1.0)).clamp(0.0, 1.0) * (source_end - source_start).max(0.0)
}

fn source_bottom_top_line_position(editor: &EditorState, section_start: f32) -> Option<f32> {
    let visible_lines = editor.visible_row_range()?.len();
    Some(
        editor
            .text()
            .lines_len()
            .saturating_sub(visible_lines)
            .max(section_start as usize) as f32,
    )
}

impl DocumentEditorView {
    fn mark_preview_driven_source_scroll(
        &mut self,
        target: PreviewSourceTarget,
        direction: f32,
        cx: &mut Context<Self>,
    ) {
        self.split_sync.preview_drives_source = true;
        self.split_sync.pending_preview_scroll_settle = Some(cx.spawn(async move |this, cx| {
            let mut previous = None::<(f32, f32)>;
            for _ in 0..PREVIEW_SCROLL_SETTLE_ATTEMPTS {
                cx.background_executor().timer(SPLIT_SYNC_DELAY).await;
                let settled = this
                    .update(cx, |this, cx| {
                        let editor = this.editor.read(cx);
                        let target_line_position = match target {
                            PreviewSourceTarget::Line(position) => position,
                            PreviewSourceTarget::BottomVisible => {
                                let Some(source_bounds) = this.analysis.source_bounds else {
                                    return false;
                                };
                                let Some(target_pixel) =
                                    source_scroll_target_for_bottom_visible(editor, source_bounds)
                                else {
                                    return false;
                                };
                                let current = editor.scroll_offset();
                                if target_pixel + current.y.as_f32() <= 1.0 {
                                    return true;
                                }
                                this.editor.update(cx, |editor, cx| {
                                    editor
                                        .set_scroll_offset(point(current.x, px(-target_pixel)), cx);
                                });
                                return false;
                            }
                            PreviewSourceTarget::NoCorrection => return true,
                        };
                        let Some(source_bounds) = this.analysis.source_bounds else {
                            return true;
                        };
                        let Some(current_line_position) =
                            source_top_line_position(editor, source_bounds)
                        else {
                            return false;
                        };
                        let remaining = target_line_position - current_line_position;
                        // Stop at the first overshoot so settling never reverses a wheel scroll.
                        if remaining * direction <= 0.0 || remaining.abs() < 0.1 {
                            return true;
                        }
                        let current = editor.scroll_offset();
                        let current_pixel = -current.y.as_f32();
                        let current_sample = (current_pixel, current_line_position);
                        let estimate = previous
                            .and_then(|previous| {
                                scroll_target_from_samples(
                                    previous,
                                    current_sample,
                                    target_line_position,
                                )
                            })
                            .or_else(|| {
                                source_scroll_target_for_line_position(
                                    editor,
                                    source_bounds,
                                    target_line_position,
                                )
                            });
                        let Some(target) = estimate else {
                            return true;
                        };
                        previous = Some(current_sample);
                        if (target - current_pixel) * direction <= 1.0 {
                            return true;
                        }
                        this.editor.update(cx, |editor, cx| {
                            editor.set_scroll_offset(point(current.x, px(-target)), cx);
                        });
                        false
                    })
                    .unwrap_or(true);
                if settled {
                    break;
                }
            }
            // Trackpad events may pause briefly while the gesture is still active.
            cx.background_executor()
                .timer(PREVIEW_SCROLL_IDLE_DELAY)
                .await;
            this.update(cx, |this, cx| {
                let editor = this.editor.read(cx);
                this.split_sync.last_source_top_pixel = Some(-editor.scroll_offset().y.as_f32());
                this.split_sync.last_source_line_position = this
                    .analysis
                    .source_bounds
                    .and_then(|bounds| source_top_line_position(editor, bounds));
                this.split_sync.preview_drives_source = false;
                this.split_sync.pending_preview_scroll_settle = None;
            })
            .ok();
        }));
    }

    pub(super) fn sync_preview_from_source(&mut self, cx: &mut Context<Self>) -> bool {
        if self.mode != EditorMode::Split || self.kind != DocumentKind::Markdown {
            return true;
        }
        let editor = self.editor.read(cx);
        if editor.line_height().is_none() {
            return false;
        }
        let top_pixel = -editor.scroll_offset().y.as_f32();
        let source_line_position = self
            .analysis
            .source_bounds
            .and_then(|bounds| source_top_line_position(editor, bounds));
        if self.split_sync.preview_drives_source {
            return true;
        }
        if self
            .split_sync
            .last_source_top_pixel
            .is_some_and(|last| !should_scroll_pixels(last, top_pixel))
            && self.split_sync.last_source_line_position == source_line_position
        {
            return true;
        }

        if self.analysis.outline_rows.is_empty() {
            let synced = self.sync_blocks_preview_from_source_pixel(cx);
            if synced {
                self.split_sync.last_source_top_pixel = Some(top_pixel);
                self.split_sync.last_source_line_position = source_line_position;
            }
            return synced;
        }
        let Some(source_line_position) = source_line_position else {
            return false;
        };
        let top = source_line_position.floor() as usize;
        let Some(target) = preview_section_for_source_line(top, &self.analysis.outline) else {
            return true;
        };
        let list = &self.analysis.preview_list_state;
        let Some(bounds) = list.bounds_for_item(target) else {
            list.scroll_to(ListOffset {
                item_ix: target,
                offset_in_item: px(0.),
            });
            cx.notify();
            return false;
        };
        let Some((source_start, source_end)) =
            section_source_line_span(target, &self.analysis.outline, editor.text().lines_len())
        else {
            return true;
        };
        let (source_end, preview_height) =
            if source_line_for_preview_section(target + 1, &self.analysis.outline).is_none() {
                (
                    source_bottom_top_line_position(editor, source_start).unwrap_or(source_end),
                    (bounds.size.height - list.viewport_bounds().size.height)
                        .as_f32()
                        .max(0.0),
                )
            } else {
                (source_end, bounds.size.height.as_f32())
            };
        let offset = preview_offset_for_source_position(
            source_line_position,
            source_start,
            source_end,
            preview_height,
        );
        let current = list.logical_scroll_top();
        if current.item_ix != target
            || should_scroll_pixels(current.offset_in_item.as_f32(), offset)
        {
            list.scroll_to(ListOffset {
                item_ix: target,
                offset_in_item: px(offset),
            });
            cx.notify();
        }
        self.split_sync.last_source_top_pixel = Some(top_pixel);
        self.split_sync.last_source_line_position = Some(source_line_position);
        self.split_sync.last_preview_scroll_top = Some(list.logical_scroll_top());
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
        let source_max = pixel_offset_for_line(total_lines, line_height.as_f32())
            - source_bounds.size.height.as_f32();
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
        self.split_sync.last_source_top_pixel = None;
        self.split_sync.last_source_line_position = None;
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

    pub(super) fn sync_source_from_preview_scroll(
        view: WeakEntity<Self>,
        wheel_direction: Option<f32>,
        cx: &mut App,
    ) {
        let Some((editor, source_bounds, source_start_line, source_end_line, list)) = view
            .read_with(cx, |this, _| {
                if this.mode != EditorMode::Split || this.kind != DocumentKind::Markdown {
                    return None;
                }
                let source_bounds = this.analysis.source_bounds?;
                let section = this
                    .analysis
                    .preview_list_state
                    .logical_scroll_top()
                    .item_ix;
                let start_line = source_line_for_preview_section(section, &this.analysis.outline)?;
                let end_line = source_line_for_preview_section(section + 1, &this.analysis.outline);
                Some((
                    this.editor.clone(),
                    source_bounds,
                    start_line,
                    end_line,
                    this.analysis.preview_list_state.clone(),
                ))
            })
            .ok()
            .flatten()
        else {
            return;
        };
        let scroll_top = list.logical_scroll_top();
        let Some(bounds) = list.bounds_for_item(scroll_top.item_ix) else {
            return;
        };
        let preview_scroll = -list.scroll_px_offset_for_scrollbar().y.as_f32();
        let preview_max = list.max_offset_for_scrollbar().y.as_f32();
        let preview_at_bottom = preview_max > 0.0 && preview_max - preview_scroll <= 1.0;
        editor.update(cx, |editor, cx| {
            let total_lines = editor.text().lines_len();
            let source_start = source_start_line as f32;
            let source_end = source_end_line
                .unwrap_or(total_lines)
                .max(source_start_line + 1) as f32;
            let (source_end, preview_height) = if source_end_line.is_none() {
                (
                    source_bottom_top_line_position(editor, source_start).unwrap_or(source_end),
                    (bounds.size.height - list.viewport_bounds().size.height)
                        .as_f32()
                        .max(0.0),
                )
            } else {
                (source_end, bounds.size.height.as_f32())
            };
            let target_line_position = source_position_for_preview_offset(
                scroll_top.offset_in_item.as_f32(),
                preview_height,
                source_start,
                source_end,
            );
            let direction = view
                .update(cx, |this, _| {
                    this.split_sync
                        .last_preview_scroll_top
                        .replace(scroll_top)
                        .map(|previous| {
                            preview_scroll_direction(previous, scroll_top, wheel_direction)
                        })
                })
                .ok()
                .flatten()
                .unwrap_or_else(|| {
                    source_top_line_position(editor, source_bounds)
                        .map_or(0.0, |position| target_line_position - position)
                })
                .signum();
            let direction = if direction == 0.0 && preview_at_bottom && wheel_direction == Some(1.0)
            {
                1.0
            } else {
                direction
            };
            if direction == 0.0 {
                return;
            }
            let Some(target) = (if preview_at_bottom {
                source_scroll_target_for_bottom_visible(editor, source_bounds)
            } else {
                source_scroll_target_for_line_position(editor, source_bounds, target_line_position)
            }) else {
                return;
            };
            let current = editor.scroll_offset();
            let current_pixel = -current.y.as_f32();
            view.update(cx, |this, cx| {
                this.mark_preview_driven_source_scroll(
                    if preview_at_bottom {
                        PreviewSourceTarget::BottomVisible
                    } else {
                        PreviewSourceTarget::Line(target_line_position)
                    },
                    direction,
                    cx,
                );
            })
            .ok();
            if (target - current_pixel) * direction > 1.0 {
                editor.set_scroll_offset(point(current.x, px(-target)), cx);
            }
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
            let source_max =
                pixel_offset_for_line(total_lines, line_height.as_f32()) - source_viewport;
            if source_max <= 0.0 {
                return;
            }
            let target = pixel_target_for_fraction(fraction, source_max);
            let current = editor.scroll_offset();
            let current_pixel = -current.y.as_f32();
            if !should_scroll_pixels(current_pixel, target) {
                return;
            }
            view.update(cx, |this, cx| {
                this.mark_preview_driven_source_scroll(PreviewSourceTarget::NoCorrection, 0.0, cx);
            })
            .ok();
            editor.set_scroll_offset(point(current.x, px(-target)), cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        pixel_offset_for_line, pixel_target_for_fraction, preview_offset_for_source_position,
        preview_scroll_direction, preview_section_for_source_line, scroll_target_from_samples,
        should_scroll_pixels, source_line_for_preview_section, source_position_for_preview_offset,
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

    #[test]
    fn preview_scroll_direction_follows_logical_position_during_reflow() {
        let earlier = gpui_kit::ListOffset {
            item_ix: 2,
            offset_in_item: gpui_kit::px(100.),
        };
        let later = gpui_kit::ListOffset {
            item_ix: 2,
            offset_in_item: gpui_kit::px(102.),
        };
        assert_eq!(preview_scroll_direction(earlier, later, None), 1.0);
        assert_eq!(preview_scroll_direction(later, earlier, None), -1.0);
        assert_eq!(
            preview_scroll_direction(
                later,
                gpui_kit::ListOffset {
                    item_ix: 3,
                    offset_in_item: gpui_kit::px(0.),
                },
                None,
            ),
            1.0
        );
        let reflowed_downward_wheel = gpui_kit::ListOffset {
            item_ix: 2,
            offset_in_item: gpui_kit::px(80.),
        };
        assert_eq!(
            preview_scroll_direction(later, reflowed_downward_wheel, Some(1.0)),
            1.0,
            "a downward trackpad event should keep its direction across remeasurement"
        );
    }

    #[test]
    fn tall_section_scroll_maps_continuously_in_both_directions() {
        let source_start = 20.0 * 18.0;
        let source_end = 120.0 * 18.0;
        let preview_height = 3_600.0;

        for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let source = source_start + (source_end - source_start) * fraction;
            let preview = preview_offset_for_source_position(
                source,
                source_start,
                source_end,
                preview_height,
            );
            assert!((preview - preview_height * fraction).abs() < 0.01);
            let restored = source_position_for_preview_offset(
                preview,
                preview_height,
                source_start,
                source_end,
            );
            assert!((restored - source).abs() < 0.01);
        }
    }

    #[test]
    fn section_mapping_clamps_out_of_range_positions() {
        assert_eq!(
            preview_offset_for_source_position(0.0, 100.0, 200.0, 500.0),
            0.0
        );
        assert_eq!(
            preview_offset_for_source_position(300.0, 100.0, 200.0, 500.0),
            500.0
        );
        assert_eq!(
            source_position_for_preview_offset(-10.0, 500.0, 100.0, 200.0),
            100.0
        );
        assert_eq!(
            source_position_for_preview_offset(600.0, 500.0, 100.0, 200.0),
            200.0
        );
    }

    #[test]
    fn source_scroll_samples_account_for_wrapped_lines() {
        assert_eq!(
            scroll_target_from_samples((0.0, 0.0), (2_000.0, 20.0), 100.0),
            Some(10_000.0)
        );
        assert_eq!(
            scroll_target_from_samples((0.0, 0.0), (2_000.0, 0.0), 100.0),
            None
        );
    }
}
