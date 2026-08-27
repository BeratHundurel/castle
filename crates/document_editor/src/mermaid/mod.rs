use self::renderer::{MermaidTheme, is_supported_diagram, render_to_svg};
use gpui::{
    App, AppContext as _, ClickEvent, ClipboardItem, Context, ElementId, Entity, FocusHandle,
    ImageSource, InteractiveElement as _, IntoElement, ParentElement as _, ParsedSvg, RenderImage,
    SMOOTH_SVG_SCALE_FACTOR, ScrollDelta, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, Window, accesskit::Role, div, img, point,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    theme::Colorize as _,
    v_flex,
};
pub(super) use parser::{MermaidDescriptor, parse_mermaid_blocks};
use parser::{is_closed_mermaid_fence, parse_mermaid_info};

mod parser;
mod renderer;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use super::DocumentEditorView;

const MAX_RENDER_JOBS: usize = 2;
const ZOOM_STEP: f32 = 0.1;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 2.0;
const ZOOM_DEBOUNCE: Duration = Duration::from_millis(300);
const SCROLL_EPSILON: f32 = 0.01;
const DIAGRAM_HORIZONTAL_PADDING: f32 = 32.0;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct CacheKey {
    source: SharedString,
    scale: u16,
    theme: u64,
}

#[derive(Clone)]
struct Raster {
    image: Arc<RenderImage>,
    zoom: f32,
    natural_width: f32,
    natural_height: f32,
}

enum CacheEntry {
    Loading {
        generation: u64,
        fallback: Option<Raster>,
    },
    Ready {
        generation: u64,
        parsed: Arc<ParsedSvg>,
        rasters: HashMap<u32, Raster>,
    },
    Failed {
        message: SharedString,
        fallback: Option<Raster>,
    },
}

#[derive(Clone)]
enum CacheSnapshot {
    Loading(Option<Raster>),
    Ready {
        raster: Raster,
    },
    Failed {
        message: SharedString,
        fallback: Option<Raster>,
    },
}

struct Presentation {
    showing_code: bool,
    zoom: f32,
    fit_to_width: bool,
    available_width: f32,
    copied_generation: u64,
    zoom_generation: u64,
    focus_handles: Option<MermaidFocusHandles>,
}

#[derive(Clone)]
struct MermaidFocusHandles {
    zoom_out: FocusHandle,
    reset: FocusHandle,
    zoom_in: FocusHandle,
    fit: FocusHandle,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            showing_code: false,
            zoom: 1.0,
            fit_to_width: false,
            available_width: 0.0,
            copied_generation: 0,
            zoom_generation: 0,
            focus_handles: None,
        }
    }
}

impl Presentation {
    fn with_focus_handles(mut self, cx: &mut Context<DocumentEditorView>) -> Self {
        if self.focus_handles.is_none() {
            self.focus_handles = Some(MermaidFocusHandles {
                zoom_out: cx.focus_handle(),
                reset: cx.focus_handle(),
                zoom_in: cx.focus_handle(),
                fit: cx.focus_handle(),
            });
        }
        self
    }
}

#[derive(Clone)]
struct PresentationSnapshot {
    showing_code: bool,
    zoom: f32,
    fit_to_width: bool,
    available_width: f32,
    copied: bool,
    focus_handles: Option<MermaidFocusHandles>,
}

#[derive(Clone)]
pub(super) struct MermaidRenderSnapshots(
    Arc<HashMap<usize, (CacheSnapshot, PresentationSnapshot)>>,
);

enum RenderRequest {
    Layout {
        key: CacheKey,
        generation: u64,
        theme: Box<MermaidTheme>,
    },
    Raster {
        key: CacheKey,
        generation: u64,
        zoom: f32,
        parsed: Arc<ParsedSvg>,
    },
}

struct CompletedRender {
    key: CacheKey,
    generation: u64,
    requested_zoom: Option<f32>,
    value: Result<(Option<Arc<ParsedSvg>>, Raster), String>,
}

#[derive(Default)]
pub(super) struct MermaidState {
    analyzed: Vec<MermaidDescriptor>,
    active: Vec<MermaidDescriptor>,
    keys_by_occurrence: HashMap<usize, CacheKey>,
    cache: HashMap<CacheKey, CacheEntry>,
    presentations: HashMap<usize, Presentation>,
    queue: VecDeque<RenderRequest>,
    pending_rasters: HashSet<(CacheKey, u32)>,
    active_jobs: usize,
    active_tasks: HashMap<u64, Task<()>>,
    images_pending_release: HashMap<gpui::ImageId, Arc<RenderImage>>,
    next_job_id: u64,
    generation: u64,
    theme_fingerprint: Option<u64>,
    remeasure_pending: bool,
}

impl MermaidState {
    pub(super) fn set_analyzed(&mut self, descriptors: Vec<MermaidDescriptor>) {
        self.analyzed = descriptors;
    }

    fn snapshot(
        &self,
        occurrence: usize,
        available_width: f32,
    ) -> Option<(CacheSnapshot, PresentationSnapshot)> {
        let key = self.keys_by_occurrence.get(&occurrence)?;
        let presentation = self.presentations.get(&occurrence)?;
        let desired_zoom = display_zoom(presentation, available_width, self.cache.get(key));
        let cache = match self.cache.get(key)? {
            CacheEntry::Loading { fallback, .. } => CacheSnapshot::Loading(fallback.clone()),
            CacheEntry::Failed {
                message, fallback, ..
            } => CacheSnapshot::Failed {
                message: message.clone(),
                fallback: fallback.clone(),
            },
            CacheEntry::Ready { rasters, .. } => {
                let desired_bits = zoom_bits(desired_zoom);
                let exact = rasters.get(&desired_bits).cloned();
                let raster = exact
                    .clone()
                    .or_else(|| rasters.get(&zoom_bits(1.0)).cloned())
                    .or_else(|| rasters.values().next().cloned())?;
                CacheSnapshot::Ready { raster }
            }
        };
        Some((
            cache,
            PresentationSnapshot {
                showing_code: presentation.showing_code,
                zoom: desired_zoom,
                fit_to_width: presentation.fit_to_width,
                available_width,
                copied: presentation.copied_generation != 0,
                focus_handles: presentation.focus_handles.clone(),
            },
        ))
    }

    pub(super) fn render_snapshots(&self, available_width: f32) -> MermaidRenderSnapshots {
        MermaidRenderSnapshots(Arc::new(
            self.keys_by_occurrence
                .keys()
                .filter_map(|occurrence| {
                    self.snapshot(*occurrence, available_width)
                        .map(|snapshot| (*occurrence, snapshot))
                })
                .collect(),
        ))
    }

    fn release_entry(entry: CacheEntry, cx: &mut App) {
        match entry {
            CacheEntry::Loading { fallback, .. } | CacheEntry::Failed { fallback, .. } => {
                if let Some(raster) = fallback {
                    cx.drop_image(raster.image, None);
                }
            }
            CacheEntry::Ready { rasters, .. } => {
                let mut seen = HashSet::new();
                for raster in rasters.into_values() {
                    if seen.insert(raster.image.id) {
                        cx.drop_image(raster.image, None);
                    }
                }
            }
        }
    }

    fn retire_image(&mut self, image: Arc<RenderImage>) {
        self.images_pending_release.insert(image.id, image);
    }

    fn retire_entry(&mut self, entry: CacheEntry) {
        match entry {
            CacheEntry::Loading { fallback, .. } | CacheEntry::Failed { fallback, .. } => {
                if let Some(raster) = fallback {
                    self.retire_image(raster.image);
                }
            }
            CacheEntry::Ready { rasters, .. } => {
                for raster in rasters.into_values() {
                    self.retire_image(raster.image);
                }
            }
        }
    }

    pub(super) fn release_retired_images_after_frame(&mut self, window: &mut Window) {
        if self.images_pending_release.is_empty() {
            return;
        }
        let images = std::mem::take(&mut self.images_pending_release);
        window.on_next_frame(move |window, cx| {
            for image in images.into_values() {
                cx.drop_image(image, Some(window));
            }
        });
    }

    pub(super) fn clear(&mut self, cx: &mut App) {
        for entry in std::mem::take(&mut self.cache).into_values() {
            Self::release_entry(entry, cx);
        }
        for image in std::mem::take(&mut self.images_pending_release).into_values() {
            cx.drop_image(image, None);
        }
        self.queue.clear();
        self.pending_rasters.clear();
        self.keys_by_occurrence.clear();
        self.presentations.clear();
        self.active.clear();
        self.active_tasks.clear();
        self.active_jobs = 0;
    }
}

#[derive(Clone)]
pub(super) struct MermaidPlugin {
    editor: Entity<DocumentEditorView>,
    section_offset: usize,
    snapshots: MermaidRenderSnapshots,
}

#[derive(Clone)]
struct MermaidBlock {
    source: SharedString,
    occurrence: usize,
}

impl MermaidPlugin {
    pub(super) fn new(
        editor: Entity<DocumentEditorView>,
        section_offset: usize,
        snapshots: MermaidRenderSnapshots,
    ) -> Self {
        Self {
            editor,
            section_offset,
            snapshots,
        }
    }
}

impl gpui_component::text::MarkdownPlugin for MermaidPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "castle-mermaid"
    }

    fn parse(
        &self,
        node: &gpui_component::text::markdown_ast::Node,
        cx: &gpui_component::text::MarkdownParseContext<'_>,
    ) -> Option<gpui_component::text::MarkdownNode> {
        use gpui_component::text::markdown_ast::Node;
        let Node::Code(code) = node else {
            return None;
        };
        let info = match (&code.lang, &code.meta) {
            (Some(language), Some(meta)) => format!("{language} {meta}"),
            (Some(language), None) => language.clone(),
            _ => return None,
        };
        parse_mermaid_info(&info)?;
        if !is_supported_diagram(&code.value) {
            return None;
        }
        let source = cx.node_source(node)?;
        if !is_closed_mermaid_fence(source) {
            return None;
        }
        let position = node.position()?;
        let occurrence = self.section_offset.saturating_add(position.start.offset);
        Some(
            gpui_component::text::MarkdownNode::new(
                self.name(),
                MermaidBlock {
                    source: code.value.clone().into(),
                    occurrence,
                },
            )
            .text(code.value.clone())
            .markdown(source),
        )
    }

    fn render(
        &self,
        node: &gpui_component::text::MarkdownNode,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let Some(block) = node.data::<MermaidBlock>() else {
            return div().into_any_element();
        };
        render_block(
            self.editor.clone(),
            block.clone(),
            self.snapshots.0.get(&block.occurrence).cloned(),
            window,
            cx,
        )
    }
}

fn theme_from_app(cx: &App) -> MermaidTheme {
    let theme = cx.theme();
    let chart_colors = [
        theme.chart_1,
        theme.chart_2,
        theme.chart_3,
        theme.chart_4,
        theme.chart_5,
    ];
    MermaidTheme {
        font_family: theme.font_family.to_string(),
        background: theme.background.to_hex(),
        surface: theme.primary.mix_oklab(theme.background, 0.08).to_hex(),
        surface_alt: theme.primary.mix_oklab(theme.background, 0.04).to_hex(),
        foreground: theme.foreground.to_hex(),
        muted_foreground: theme.foreground.mix_oklab(theme.background, 0.62).to_hex(),
        border: theme.border.to_hex(),
        primary: theme.primary.to_hex(),
        warning: theme.warning.to_hex(),
        danger: theme.danger.to_hex(),
        success: theme.success.to_hex(),
        chart_palette: chart_colors.iter().map(|color| color.to_hex()).collect(),
        accent_surfaces: chart_colors
            .iter()
            .map(|color| color.mix_oklab(theme.background, 0.16).to_hex())
            .collect(),
    }
}

fn zoom_bits(zoom: f32) -> u32 {
    (zoom * 1000.0).round().to_bits()
}

fn fit_zoom(available_width: f32, entry: Option<&CacheEntry>) -> f32 {
    let natural_width = match entry {
        Some(CacheEntry::Ready { rasters, .. }) => {
            rasters.values().next().map(|raster| raster.natural_width)
        }
        Some(CacheEntry::Loading {
            fallback: Some(raster),
            ..
        })
        | Some(CacheEntry::Failed {
            fallback: Some(raster),
            ..
        }) => Some(raster.natural_width),
        _ => None,
    };
    natural_width
        .filter(|width| *width > 0.0 && available_width > 0.0)
        .map(|width| (available_width / width).min(1.0))
        .unwrap_or(1.0)
}

fn display_zoom(
    presentation: &Presentation,
    available_width: f32,
    entry: Option<&CacheEntry>,
) -> f32 {
    if presentation.fit_to_width {
        fit_zoom(available_width, entry)
    } else {
        presentation.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }
}

fn render_block(
    editor: Entity<DocumentEditorView>,
    block: MermaidBlock,
    snapshot: Option<(CacheSnapshot, PresentationSnapshot)>,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    let Some((cache, presentation)) = snapshot else {
        return render_source(&block.source, Some("Rendering…"), cx);
    };

    let copied_label = if presentation.copied {
        "Copied"
    } else {
        "Copy"
    };
    let scroll_handle = window
        .use_keyed_state(("mermaid-scroll-state", block.occurrence), cx, |_, _| {
            ScrollHandle::new()
        })
        .read(cx)
        .clone();
    let zoom_controls = (!presentation.showing_code).then(|| {
        let occurrence = block.occurrence;
        let available_width = presentation.available_width;
        h_flex()
            .gap_0p5()
            .child(mermaid_zoom_control(
                ("zoom-out-mermaid", occurrence),
                ("−", "Zoom out"),
                presentation.zoom > MIN_ZOOM,
                false,
                presentation
                    .focus_handles
                    .as_ref()
                    .map(|handles| handles.zoom_out.clone()),
                cx,
                {
                    let editor = editor.clone();
                    move |_, _, cx| {
                        editor.update(cx, |this, cx| this.zoom_mermaid(occurrence, -ZOOM_STEP, cx));
                    }
                },
            ))
            .child(mermaid_zoom_control(
                ("reset-mermaid-zoom", occurrence),
                (
                    format!("{}%", (presentation.zoom * 100.0).round() as u32),
                    "Reset to 100%",
                ),
                true,
                false,
                presentation
                    .focus_handles
                    .as_ref()
                    .map(|handles| handles.reset.clone()),
                cx,
                {
                    let editor = editor.clone();
                    move |_, _, cx| {
                        editor.update(cx, |this, cx| this.reset_mermaid_zoom(occurrence, cx));
                    }
                },
            ))
            .child(mermaid_zoom_control(
                ("zoom-in-mermaid", occurrence),
                ("+", "Zoom in"),
                presentation.zoom < MAX_ZOOM,
                false,
                presentation
                    .focus_handles
                    .as_ref()
                    .map(|handles| handles.zoom_in.clone()),
                cx,
                {
                    let editor = editor.clone();
                    move |_, _, cx| {
                        editor.update(cx, |this, cx| this.zoom_mermaid(occurrence, ZOOM_STEP, cx));
                    }
                },
            ))
            .child(mermaid_zoom_control(
                ("fit-mermaid-zoom", occurrence),
                ("Fit", "Fit diagram to width"),
                true,
                presentation.fit_to_width,
                presentation
                    .focus_handles
                    .as_ref()
                    .map(|handles| handles.fit.clone()),
                cx,
                {
                    let editor = editor.clone();
                    move |_, _, cx| {
                        editor.update(cx, |this, cx| {
                            this.fit_mermaid_to_width(occurrence, available_width, cx)
                        });
                    }
                },
            ))
    });
    let header = h_flex()
        .h_9()
        .px_2()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.7))
        .child(
            h_flex()
                .gap_1()
                .child(
                    Button::new(("mermaid-preview-tab", block.occurrence))
                        .label("Preview")
                        .ghost()
                        .xsmall()
                        .selected(!presentation.showing_code)
                        .on_click({
                            let editor = editor.clone();
                            move |_, _, cx| {
                                editor.update(cx, |this, cx| {
                                    this.set_mermaid_code_visible(block.occurrence, false, cx)
                                });
                            }
                        }),
                )
                .child(
                    Button::new(("mermaid-code-tab", block.occurrence))
                        .label("Code")
                        .ghost()
                        .xsmall()
                        .selected(presentation.showing_code)
                        .on_click({
                            let editor = editor.clone();
                            move |_, _, cx| {
                                editor.update(cx, |this, cx| {
                                    this.set_mermaid_code_visible(block.occurrence, true, cx)
                                });
                            }
                        }),
                ),
        )
        .child(
            h_flex().gap_1().children(zoom_controls).child(
                Button::new(("copy-mermaid", block.occurrence))
                    .label(copied_label)
                    .ghost()
                    .xsmall()
                    .on_click({
                        let editor = editor.clone();
                        let source = block.source.clone();
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(source.to_string()));
                            editor.update(cx, |this, cx| {
                                this.mark_mermaid_copied(block.occurrence, cx)
                            });
                        }
                    }),
            ),
        );

    let body = if presentation.showing_code {
        render_source(&block.source, None, cx)
    } else {
        match cache {
            CacheSnapshot::Loading(fallback) => fallback.map_or_else(
                || render_source(&block.source, Some("Rendering…"), cx),
                |raster| {
                    render_image_body(
                        editor.clone(),
                        &block,
                        raster,
                        &presentation,
                        &scroll_handle,
                        cx,
                    )
                },
            ),
            CacheSnapshot::Failed { message, fallback } => {
                let error = v_flex()
                    .gap_2()
                    .p_3()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("Could not render diagram: {message}"))
                    .child(
                        Button::new(("retry-mermaid", block.occurrence))
                            .label("Retry")
                            .outline()
                            .xsmall()
                            .on_click({
                                let editor = editor.clone();
                                move |_, _, cx| {
                                    editor.update(cx, |this, cx| {
                                        this.retry_mermaid(block.occurrence, cx)
                                    });
                                }
                            }),
                    );
                let content = if let Some(raster) = fallback {
                    render_image_body(
                        editor.clone(),
                        &block,
                        raster,
                        &presentation,
                        &scroll_handle,
                        cx,
                    )
                } else {
                    render_source(&block.source, None, cx)
                };
                v_flex().child(content).child(error).into_any_element()
            }
            CacheSnapshot::Ready { raster } => render_image_body(
                editor.clone(),
                &block,
                raster,
                &presentation,
                &scroll_handle,
                cx,
            ),
        }
    };
    v_flex()
        .id(("mermaid-diagram", block.occurrence))
        .w_full()
        .my_3()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary.opacity(0.12))
        .overflow_hidden()
        .child(header)
        .child(body)
        .into_any_element()
}

fn mermaid_zoom_control(
    id: impl Into<ElementId>,
    content: (impl Into<SharedString>, &'static str),
    enabled: bool,
    selected: bool,
    focus_handle: Option<FocusHandle>,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    let (label, aria_label) = content;
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(aria_label)
        .h_5()
        .min_w_5()
        .px_1()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(cx.theme().radius)
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .when(selected, |this| {
            this.bg(cx.theme().primary.opacity(0.14))
                .text_color(cx.theme().primary)
        })
        .when_some(enabled.then_some(focus_handle).flatten(), |this, handle| {
            this.track_focus(&handle.tab_stop(true))
        })
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(|style| style.bg(cx.theme().secondary.opacity(0.5)))
                .on_click(on_click)
        })
        .when(!enabled, |this| this.opacity(0.4))
        .child(label.into())
        .into_any_element()
}

fn render_source(
    source: &SharedString,
    status: Option<&'static str>,
    cx: &App,
) -> gpui::AnyElement {
    v_flex()
        .relative()
        .p_3()
        .gap_2()
        .bg(cx.theme().muted.opacity(0.22))
        .font_family(cx.theme().mono_font_family.clone())
        .text_sm()
        .child(source.clone())
        .children(status.map(|status| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(status)
        }))
        .into_any_element()
}

fn render_image_body(
    editor: Entity<DocumentEditorView>,
    block: &MermaidBlock,
    raster: Raster,
    presentation: &PresentationSnapshot,
    scroll_handle: &ScrollHandle,
    cx: &App,
) -> gpui::AnyElement {
    let display_width = raster.natural_width * presentation.zoom;
    let display_height = raster.natural_height * presentation.zoom;
    let occurrence = block.occurrence;
    let image = img(ImageSource::Render(raster.image))
        .w(px(display_width))
        .h(px(display_height));
    let mut scroll = div()
        .id(("mermaid-scroll", occurrence))
        .relative()
        .w_full()
        .overflow_x_scroll()
        .track_scroll(scroll_handle)
        .on_scroll_wheel({
            let editor = editor.clone();
            let scroll_handle = scroll_handle.clone();
            move |event: &ScrollWheelEvent, window, cx| {
                if !(event.modifiers.control || event.modifiers.platform) {
                    let delta = event.delta.pixel_delta(window.line_height());
                    let shifted_delta = shifted_horizontal_delta(
                        f32::from(delta.x),
                        f32::from(delta.y),
                        event.modifiers.shift,
                    );
                    if shifted_delta
                        .is_some_and(|delta| scroll_mermaid_horizontally(&scroll_handle, delta))
                    {
                        editor.update(cx, |_, cx| cx.notify());
                        cx.stop_propagation();
                    }
                    return;
                }
                let ticks = match event.delta {
                    ScrollDelta::Lines(lines) => lines.y,
                    ScrollDelta::Pixels(pixels) => f32::from(pixels.y) / 20.0,
                }
                .clamp(-1.0, 1.0);
                if ticks != 0.0 {
                    editor.update(cx, |this, cx| {
                        this.zoom_mermaid(occurrence, ticks * ZOOM_STEP, cx)
                    });
                }
                cx.stop_propagation();
            }
        })
        .child(
            div()
                .w(px(display_width + DIAGRAM_HORIZONTAL_PADDING))
                .min_w_full()
                .flex_shrink_0()
                .p_4()
                .bg(cx.theme().background.opacity(0.72))
                .child(image),
        );
    scroll.style().restrict_scroll_to_axis = Some(true);

    v_flex()
        .relative()
        .child(scroll)
        .horizontal_scrollbar(scroll_handle)
        .into_any_element()
}

fn shifted_horizontal_delta(x: f32, y: f32, shift: bool) -> Option<f32> {
    if !shift {
        return None;
    }
    let delta = if y.abs() > SCROLL_EPSILON { y } else { x };
    (delta.abs() > SCROLL_EPSILON).then_some(delta)
}

fn scroll_mermaid_horizontally(scroll_handle: &ScrollHandle, delta: f32) -> bool {
    let offset = scroll_handle.offset();
    let Some(next_x) = next_horizontal_scroll_offset(
        f32::from(offset.x),
        f32::from(scroll_handle.max_offset().x),
        delta,
    ) else {
        return false;
    };
    scroll_handle.set_offset(point(px(next_x), offset.y));
    true
}

fn next_horizontal_scroll_offset(current: f32, max: f32, delta: f32) -> Option<f32> {
    if max <= SCROLL_EPSILON || delta.abs() <= SCROLL_EPSILON {
        return None;
    }
    let next = (current + delta).clamp(-max, 0.0);
    ((next - current).abs() > SCROLL_EPSILON).then_some(next)
}

impl DocumentEditorView {
    pub(super) fn activate_mermaids(&mut self, cx: &mut Context<Self>) {
        let theme = theme_from_app(cx);
        let fingerprint = theme.fingerprint();
        let descriptors = self.mermaid.analyzed.clone();
        self.mermaid.generation = self.mermaid.generation.saturating_add(1);
        let generation = self.mermaid.generation;

        let old_active = std::mem::take(&mut self.mermaid.active);
        let old_presentations = std::mem::take(&mut self.mermaid.presentations);
        let old_keys_by_occurrence = self.mermaid.keys_by_occurrence.clone();
        self.mermaid.queue.clear();
        self.mermaid.pending_rasters.clear();
        let mut presentation_buckets: HashMap<(SharedString, u16), VecDeque<Presentation>> =
            HashMap::new();
        for descriptor in &old_active {
            if let Some(presentation) = old_presentations.get(&descriptor.range.start) {
                presentation_buckets
                    .entry((descriptor.source.clone(), descriptor.scale))
                    .or_default()
                    .push_back(Presentation {
                        showing_code: presentation.showing_code,
                        zoom: presentation.zoom,
                        fit_to_width: presentation.fit_to_width,
                        available_width: presentation.available_width,
                        copied_generation: 0,
                        zoom_generation: presentation.zoom_generation,
                        focus_handles: presentation.focus_handles.clone(),
                    });
            }
        }

        self.mermaid.keys_by_occurrence.clear();
        let wanted = descriptors
            .iter()
            .map(|descriptor| CacheKey {
                source: descriptor.source.clone(),
                scale: descriptor.scale,
                theme: fingerprint,
            })
            .collect::<HashSet<_>>();
        let mut fallbacks = HashMap::new();
        for (index, descriptor) in descriptors.iter().enumerate() {
            let key = CacheKey {
                source: descriptor.source.clone(),
                scale: descriptor.scale,
                theme: fingerprint,
            };
            if !self.mermaid.cache.contains_key(&key)
                && !fallbacks.contains_key(&key)
                && let Some(old_descriptor) = old_active.get(index)
                && let Some(old_key) = old_keys_by_occurrence.get(&old_descriptor.range.start)
                && old_key != &key
                && !wanted.contains(old_key)
                && let Some(old_entry) = self.mermaid.cache.remove(old_key)
                && let Some(raster) = take_entry_raster(old_entry, &mut self.mermaid)
            {
                fallbacks.insert(key, raster);
            }
        }

        for descriptor in &descriptors {
            let key = CacheKey {
                source: descriptor.source.clone(),
                scale: descriptor.scale,
                theme: fingerprint,
            };
            self.mermaid
                .keys_by_occurrence
                .insert(descriptor.range.start, key.clone());
            let presentation = presentation_buckets
                .get_mut(&(descriptor.source.clone(), descriptor.scale))
                .and_then(VecDeque::pop_front)
                .unwrap_or_default()
                .with_focus_handles(cx);
            self.mermaid
                .presentations
                .insert(descriptor.range.start, presentation);

            if !self.mermaid.cache.contains_key(&key) {
                let fallback = fallbacks.remove(&key);
                self.mermaid.cache.insert(
                    key.clone(),
                    CacheEntry::Loading {
                        generation,
                        fallback,
                    },
                );
                self.mermaid.queue.push_back(RenderRequest::Layout {
                    key,
                    generation,
                    theme: Box::new(theme.clone()),
                });
            }
        }
        self.mermaid.active = descriptors;
        self.mermaid.theme_fingerprint = Some(fingerprint);

        let obsolete = self
            .mermaid
            .cache
            .keys()
            .filter(|key| !wanted.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in obsolete {
            if let Some(entry) = self.mermaid.cache.remove(&key) {
                self.mermaid.retire_entry(entry);
            }
        }
        self.pump_mermaid_queue(cx);
        cx.notify();
    }

    fn pump_mermaid_queue(&mut self, cx: &mut Context<Self>) {
        while self.mermaid.active_jobs < MAX_RENDER_JOBS {
            let Some(request) = self.mermaid.queue.pop_front() else {
                break;
            };
            self.mermaid.active_jobs += 1;
            self.mermaid.next_job_id = self.mermaid.next_job_id.saturating_add(1);
            let job_id = self.mermaid.next_job_id;
            let svg_renderer = cx.svg_renderer();
            let task = cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move {
                        match request {
                            RenderRequest::Layout {
                                key,
                                generation,
                                theme,
                            } => {
                                let value = (|| {
                                    let svg = render_to_svg(&key.source, &theme)
                                        .map_err(|error| error.to_string())?;
                                    let parsed = Arc::new(
                                        svg_renderer
                                            .parse_svg(svg.as_bytes())
                                            .map_err(|error| error.to_string())?,
                                    );
                                    let raster = rasterize(&svg_renderer, &parsed, key.scale, 1.0)?;
                                    Ok::<_, String>((Some(parsed), raster))
                                })();
                                CompletedRender {
                                    key,
                                    generation,
                                    requested_zoom: None,
                                    value,
                                }
                            }
                            RenderRequest::Raster {
                                key,
                                generation,
                                zoom,
                                parsed,
                            } => {
                                let value = rasterize(&svg_renderer, &parsed, key.scale, zoom)
                                    .map(|raster| (None, raster));
                                CompletedRender {
                                    key,
                                    generation,
                                    requested_zoom: Some(zoom),
                                    value,
                                }
                            }
                        }
                    })
                    .await;
                this.update(cx, |this, cx| {
                    this.mermaid.active_tasks.remove(&job_id);
                    this.finish_mermaid_job(result, cx);
                    this.pump_mermaid_queue(cx);
                })
                .ok();
            });
            self.mermaid.active_tasks.insert(job_id, task);
        }
    }

    pub(super) fn deactivate_mermaids(&mut self, _cx: &mut Context<Self>) {
        self.mermaid.generation = self.mermaid.generation.saturating_add(1);
        self.mermaid.queue.clear();
        self.mermaid.pending_rasters.clear();
        self.mermaid.active_tasks.clear();
        self.mermaid.active_jobs = 0;
        let loading = self
            .mermaid
            .cache
            .keys()
            .filter(|key| {
                matches!(
                    self.mermaid.cache.get(*key),
                    Some(CacheEntry::Loading { .. })
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        for key in loading {
            if let Some(entry) = self.mermaid.cache.remove(&key) {
                self.mermaid.retire_entry(entry);
            }
        }
    }

    fn finish_mermaid_job(&mut self, result: CompletedRender, cx: &mut Context<Self>) {
        self.mermaid.active_jobs = self.mermaid.active_jobs.saturating_sub(1);
        let CompletedRender {
            key,
            generation,
            requested_zoom,
            value,
        } = result;
        if let Some(zoom) = requested_zoom {
            self.mermaid
                .pending_rasters
                .remove(&(key.clone(), zoom_bits(zoom)));
        }
        match value {
            Ok((parsed, raster)) => {
                self.mermaid
                    .pending_rasters
                    .remove(&(key.clone(), zoom_bits(raster.zoom)));
                let Some(entry) = self.mermaid.cache.get_mut(&key) else {
                    cx.drop_image(raster.image, None);
                    return;
                };
                if let Some(parsed) = parsed {
                    if let CacheEntry::Loading {
                        generation: current,
                        fallback,
                    } = entry
                        && *current == generation
                    {
                        let retired_image = fallback.take().map(|old| old.image);
                        let mut rasters = HashMap::new();
                        rasters.insert(zoom_bits(raster.zoom), raster);
                        *entry = CacheEntry::Ready {
                            generation,
                            parsed,
                            rasters,
                        };
                        if let Some(image) = retired_image {
                            self.mermaid.retire_image(image);
                        }
                    } else {
                        cx.drop_image(raster.image, None);
                    }
                } else if let CacheEntry::Ready {
                    generation: current,
                    rasters,
                    ..
                } = entry
                    && *current == generation
                {
                    let retired_image = rasters
                        .insert(zoom_bits(raster.zoom), raster)
                        .map(|old| old.image);
                    if let Some(image) = retired_image {
                        self.mermaid.retire_image(image);
                    }
                } else {
                    cx.drop_image(raster.image, None);
                }
            }
            Err(message) => {
                if requested_zoom.is_none()
                    && let Some(CacheEntry::Loading {
                        generation: current,
                        ..
                    }) = self.mermaid.cache.get(&key)
                    && *current == generation
                    && let Some(CacheEntry::Loading { fallback, .. }) =
                        self.mermaid.cache.remove(&key)
                {
                    self.mermaid.cache.insert(
                        key,
                        CacheEntry::Failed {
                            message: message.into(),
                            fallback,
                        },
                    );
                }
            }
        }
        self.schedule_mermaid_remeasure(cx);
    }

    pub(super) fn set_mermaid_code_visible(
        &mut self,
        occurrence: usize,
        visible: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(presentation) = self.mermaid.presentations.get_mut(&occurrence)
            && presentation.showing_code != visible
        {
            presentation.showing_code = visible;
            self.schedule_mermaid_remeasure(cx);
        }
    }

    pub(super) fn zoom_mermaid(&mut self, occurrence: usize, delta: f32, cx: &mut Context<Self>) {
        let current = self
            .mermaid
            .keys_by_occurrence
            .get(&occurrence)
            .and_then(|key| {
                self.mermaid
                    .presentations
                    .get(&occurrence)
                    .map(|presentation| {
                        display_zoom(
                            presentation,
                            presentation.available_width,
                            self.mermaid.cache.get(key),
                        )
                    })
            })
            .unwrap_or(1.0);
        let Some(presentation) = self.mermaid.presentations.get_mut(&occurrence) else {
            return;
        };
        let mut zoom = (current + delta).clamp(MIN_ZOOM, MAX_ZOOM);
        if (zoom - 1.0).abs() <= 0.05 {
            zoom = 1.0;
        }
        presentation.zoom = zoom;
        presentation.fit_to_width = false;
        presentation.zoom_generation = presentation.zoom_generation.saturating_add(1);
        let zoom_generation = presentation.zoom_generation;
        self.schedule_mermaid_remeasure(cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ZOOM_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                let current = this.mermaid.presentations.get(&occurrence);
                if current
                    .is_some_and(|presentation| presentation.zoom_generation == zoom_generation)
                {
                    this.queue_mermaid_raster(occurrence, zoom, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn reset_mermaid_zoom(&mut self, occurrence: usize, cx: &mut Context<Self>) {
        if let Some(presentation) = self.mermaid.presentations.get_mut(&occurrence) {
            presentation.zoom = 1.0;
            presentation.fit_to_width = false;
            presentation.zoom_generation = presentation.zoom_generation.saturating_add(1);
            self.schedule_mermaid_remeasure(cx);
            self.queue_mermaid_raster(occurrence, 1.0, cx);
        }
    }

    pub(super) fn fit_mermaid_to_width(
        &mut self,
        occurrence: usize,
        available_width: f32,
        cx: &mut Context<Self>,
    ) {
        let zoom = self
            .mermaid
            .keys_by_occurrence
            .get(&occurrence)
            .map(|key| fit_zoom(available_width, self.mermaid.cache.get(key)))
            .unwrap_or(1.0);
        if let Some(presentation) = self.mermaid.presentations.get_mut(&occurrence) {
            presentation.available_width = available_width;
            presentation.fit_to_width = true;
            presentation.zoom_generation = presentation.zoom_generation.saturating_add(1);
            self.schedule_mermaid_remeasure(cx);
            self.queue_mermaid_raster(occurrence, zoom, cx);
        }
    }

    fn queue_mermaid_raster(&mut self, occurrence: usize, zoom: f32, cx: &mut Context<Self>) {
        let Some(key) = self.mermaid.keys_by_occurrence.get(&occurrence).cloned() else {
            return;
        };
        let Some(CacheEntry::Ready {
            generation,
            parsed,
            rasters,
        }) = self.mermaid.cache.get(&key)
        else {
            return;
        };
        let bits = zoom_bits(zoom);
        if rasters.contains_key(&bits) || !self.mermaid.pending_rasters.insert((key.clone(), bits))
        {
            return;
        }
        self.mermaid.queue.push_back(RenderRequest::Raster {
            key,
            generation: *generation,
            zoom,
            parsed: parsed.clone(),
        });
        self.pump_mermaid_queue(cx);
    }

    pub(super) fn retry_mermaid(&mut self, occurrence: usize, cx: &mut Context<Self>) {
        let Some(key) = self.mermaid.keys_by_occurrence.get(&occurrence).cloned() else {
            return;
        };
        let fallback = self
            .mermaid
            .cache
            .remove(&key)
            .and_then(|entry| match entry {
                CacheEntry::Failed { fallback, .. } => fallback,
                other => {
                    self.mermaid.retire_entry(other);
                    None
                }
            });
        self.mermaid.generation = self.mermaid.generation.saturating_add(1);
        let generation = self.mermaid.generation;
        self.mermaid.cache.insert(
            key.clone(),
            CacheEntry::Loading {
                generation,
                fallback,
            },
        );
        self.mermaid.queue.push_back(RenderRequest::Layout {
            key,
            generation,
            theme: Box::new(theme_from_app(cx)),
        });
        self.pump_mermaid_queue(cx);
        cx.notify();
    }

    pub(super) fn mark_mermaid_copied(&mut self, occurrence: usize, cx: &mut Context<Self>) {
        let Some(presentation) = self.mermaid.presentations.get_mut(&occurrence) else {
            return;
        };
        presentation.copied_generation = presentation.copied_generation.saturating_add(1);
        let generation = presentation.copied_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1_200))
                .await;
            this.update(cx, |this, cx| {
                if let Some(presentation) = this.mermaid.presentations.get_mut(&occurrence)
                    && presentation.copied_generation == generation
                {
                    presentation.copied_generation = 0;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn schedule_mermaid_remeasure(&mut self, cx: &mut Context<Self>) {
        if self.mermaid.remeasure_pending {
            return;
        }
        self.mermaid.remeasure_pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            this.update(cx, |this, cx| {
                this.mermaid.remeasure_pending = false;
                this.analysis.preview_list_state.remeasure();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn take_entry_raster(entry: CacheEntry, state: &mut MermaidState) -> Option<Raster> {
    match entry {
        CacheEntry::Ready { mut rasters, .. } => {
            let raster = rasters.remove(&zoom_bits(1.0)).or_else(|| {
                rasters
                    .keys()
                    .next()
                    .copied()
                    .and_then(|key| rasters.remove(&key))
            });
            for unused in rasters.into_values() {
                state.retire_image(unused.image);
            }
            raster
        }
        CacheEntry::Loading { fallback, .. } | CacheEntry::Failed { fallback, .. } => fallback,
    }
}

fn rasterize(
    renderer: &gpui::SvgRenderer,
    parsed: &Arc<ParsedSvg>,
    fence_scale: u16,
    zoom: f32,
) -> Result<Raster, String> {
    let image = renderer
        .render_parsed(parsed, f32::from(fence_scale) / 100.0 * zoom)
        .map_err(|error| error.to_string())?;
    let size = image.size(0);
    let divisor = SMOOTH_SVG_SCALE_FACTOR * zoom;
    Ok(Raster {
        image,
        zoom,
        natural_width: size.width.0 as f32 / divisor,
        natural_height: size.height.0 as f32 / divisor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_closed_supported_mermaid_fences() {
        let source = "before\n```mermaid 50\nflowchart LR\nA-->B\n```\nafter";
        let diagrams = parse_mermaid_blocks(source);
        assert_eq!(diagrams.len(), 1);
        assert_eq!(diagrams[0].scale, 50);
        assert_eq!(diagrams[0].source, "flowchart LR\nA-->B");
        assert_eq!(
            &source[diagrams[0].range.clone()],
            "```mermaid 50\nflowchart LR\nA-->B\n```\n"
        );

        assert!(parse_mermaid_blocks("```mermaid\nflowchart LR\nA-->B").is_empty());
        assert!(parse_mermaid_blocks("```mermaid\nsankey-beta\na,b,1\n```\n").is_empty());
        assert!(parse_mermaid_blocks("~~~mermaid\nflowchart LR\nA-->B\n~~~\n").is_empty());
    }

    #[test]
    fn parses_and_clamps_scale_metadata() {
        assert_eq!(parse_mermaid_info("mermaid"), Some(100));
        assert_eq!(parse_mermaid_info("mermaid 1"), Some(10));
        assert_eq!(parse_mermaid_info("mermaid 900"), Some(500));
        assert_eq!(parse_mermaid_info("mermaid nope"), Some(100));
        assert_eq!(parse_mermaid_info("mermaid 100 extra"), Some(100));
        assert_eq!(parse_mermaid_info("rust"), None);
    }

    #[test]
    fn zoom_bounds_and_snap_are_stable() {
        let mut presentation = Presentation::default();
        assert!(!presentation.fit_to_width);
        assert_eq!(presentation.zoom, 1.0);
        presentation.zoom = 0.94;
        let next = (presentation.zoom + ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM);
        assert!((next - 1.0).abs() <= 0.05);
        assert_eq!((2.0 + ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM), MAX_ZOOM);
        assert_eq!((MIN_ZOOM - ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM), MIN_ZOOM);
    }

    #[test]
    fn horizontal_scroll_moves_and_releases_wheel_at_edges() {
        assert_eq!(
            next_horizontal_scroll_offset(0.0, 500.0, -40.0),
            Some(-40.0)
        );
        assert_eq!(
            next_horizontal_scroll_offset(-480.0, 500.0, -40.0),
            Some(-500.0)
        );
        assert_eq!(next_horizontal_scroll_offset(-500.0, 500.0, -40.0), None);
        assert_eq!(next_horizontal_scroll_offset(0.0, 500.0, 40.0), None);
        assert_eq!(next_horizontal_scroll_offset(-200.0, 0.0, -40.0), None);
    }

    #[test]
    fn only_shift_converts_vertical_wheel_input_to_diagram_scroll() {
        assert_eq!(shifted_horizontal_delta(0.0, -40.0, false), None);
        assert_eq!(shifted_horizontal_delta(0.0, -40.0, true), Some(-40.0));
        assert_eq!(shifted_horizontal_delta(-12.0, 0.0, true), Some(-12.0));
    }

    #[test]
    fn analysis_does_not_start_rendering_work() {
        let mut state = MermaidState::default();
        state.set_analyzed(parse_mermaid_blocks(
            "```mermaid\nflowchart LR\nA-->B\n```\n",
        ));
        assert_eq!(state.analyzed.len(), 1);
        assert!(state.active.is_empty());
        assert!(state.queue.is_empty());
        assert_eq!(state.active_jobs, 0);
        assert!(state.active_tasks.is_empty());
    }

    #[test]
    fn retired_images_stay_owned_until_the_next_frame() {
        let mut state = MermaidState::default();
        let image = Arc::new(RenderImage::new(Vec::<image::Frame>::new()));
        let image_id = image.id;

        state.retire_image(image.clone());
        state.retire_image(image);

        assert_eq!(state.images_pending_release.len(), 1);
        assert!(state.images_pending_release.contains_key(&image_id));
    }

    #[test]
    fn ignores_mermaid_text_nested_in_other_fences() {
        let source = "```rust\n```mermaid\nflowchart LR\nA-->B\n```\n```\n";
        assert!(parse_mermaid_blocks(source).is_empty());

        let source = "~~~text\n```mermaid\nflowchart LR\nA-->B\n```\n~~~\n";
        assert!(parse_mermaid_blocks(source).is_empty());
    }

    #[test]
    fn representative_svg_output_parses_and_rasterizes_with_gpui() {
        let theme = MermaidTheme {
            font_family: "Arial, sans-serif".into(),
            background: "#ffffff".into(),
            surface: "#f8fafc".into(),
            surface_alt: "#eef2f7".into(),
            foreground: "#172033".into(),
            muted_foreground: "#64748b".into(),
            border: "#cbd5e1".into(),
            primary: "#2563eb".into(),
            warning: "#d97706".into(),
            danger: "#dc2626".into(),
            success: "#16a34a".into(),
            chart_palette: vec!["#2563eb".into(), "#7c3aed".into(), "#0891b2".into()],
            accent_surfaces: vec!["#dbeafe".into(), "#ede9fe".into(), "#cffafe".into()],
        };
        let samples = [
            "flowchart LR\nCapture[Capture an idea] --> Draft[Write a note]\nDraft --> Review{Ready to organize?}\nReview -->|Yes| Project[Move into a project]\nReview -->|Not yet| Draft\nProject --> Link[Link related notes and tasks]\nLink --> Done[Keep the knowledge discoverable]",
            "sequenceDiagram\nactor User\nparticipant Agent\nparticipant MCP as Castle MCP\nparticipant Castle\nUser->>Agent: Create a Markdown note\nAgent->>MCP: create_note(title, content)\nMCP->>Castle: Persist note\nCastle-->>MCP: Note details\nMCP-->>Agent: Creation confirmed\nAgent-->>User: Note is ready",
            "classDiagram\nA <|-- B",
            "erDiagram\nA ||--o{ B : owns",
            "gantt\ntitle Work\nsection Build\nRender :a1, 2026-01-01, 1d",
            "pie\n\"A\" : 1\n\"B\" : 2",
            "mindmap\n root((Castle))\n  Notes",
            "stateDiagram-v2\n[*] --> Ready",
        ];
        let renderer = gpui::SvgRenderer::new(Arc::new(()));
        for source in samples {
            let svg = render_to_svg(source, &theme)
                .unwrap_or_else(|error| panic!("failed to render {source}: {error}"));
            let parsed = renderer
                .parse_svg(svg.as_bytes())
                .unwrap_or_else(|error| panic!("failed to parse {source}: {error}"));
            let image = renderer
                .render_parsed(&parsed, 1.0)
                .unwrap_or_else(|error| panic!("failed to rasterize {source}: {error}"));
            assert!(image.size(0).width.0 > 0);
            assert!(image.size(0).height.0 > 0);
            assert!(image.size(0).width.0 < 5_000);
            assert!(image.size(0).height.0 < 5_000);
        }
        assert!(render_to_svg("not-a-diagram", &theme).is_err());
    }
}
