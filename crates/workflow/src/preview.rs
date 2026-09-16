use super::*;
use gpui_kit::{ClipboardItem, ImageSource, RenderImage, SMOOTH_SVG_SCALE_FACTOR, img};
use mermaid_renderer::MermaidTheme;

#[derive(Clone)]
struct PreviewRaster {
    image: Arc<RenderImage>,
    width: f32,
    height: f32,
}

#[derive(Default)]
pub(super) struct WorkflowMermaidPreview {
    revision: u64,
    source: SharedString,
    theme_fingerprint: Option<u64>,
    rendering: bool,
    raster: Option<PreviewRaster>,
    error: Option<SharedString>,
    retired_images: Vec<Arc<RenderImage>>,
}

impl WorkflowWorkspace {
    pub(super) fn prepare_mermaid_preview(&mut self, cx: &mut Context<Self>) {
        let source: SharedString = workflow::to_mermaid(&self.draft.definition).into();
        let theme = MermaidTheme::from_app(cx);
        let theme_fingerprint = theme.fingerprint();
        if self.mermaid_preview.source == source
            && self.mermaid_preview.theme_fingerprint == Some(theme_fingerprint)
            && (self.mermaid_preview.rendering
                || self.mermaid_preview.raster.is_some()
                || self.mermaid_preview.error.is_some())
        {
            return;
        }

        self.mermaid_preview.revision = self.mermaid_preview.revision.saturating_add(1);
        let revision = self.mermaid_preview.revision;
        self.mermaid_preview.source = source.clone();
        self.mermaid_preview.theme_fingerprint = Some(theme_fingerprint);
        self.mermaid_preview.rendering = true;
        self.mermaid_preview.error = None;

        let renderer = theme.prepare();
        let svg_renderer = cx.svg_renderer();
        let task = cx.background_spawn(async move {
            let svg = renderer
                .render_to_svg(&source)
                .map_err(|error| error.to_string())?;
            let parsed = svg_renderer
                .parse_svg(svg.as_bytes())
                .map_err(|error| error.to_string())?;
            let image = svg_renderer
                .render_parsed(&parsed, 1.0)
                .map_err(|error| error.to_string())?;
            let size = image.size(0);
            Ok::<_, String>(PreviewRaster {
                image,
                width: size.width.0 as f32 / SMOOTH_SVG_SCALE_FACTOR,
                height: size.height.0 as f32 / SMOOTH_SVG_SCALE_FACTOR,
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if this.mermaid_preview.revision != revision {
                    if let Ok(raster) = result {
                        cx.drop_image(raster.image, None);
                    }
                    return;
                }
                this.mermaid_preview.rendering = false;
                match result {
                    Ok(raster) => {
                        if let Some(previous) = this.mermaid_preview.raster.replace(raster) {
                            this.mermaid_preview.retired_images.push(previous.image);
                        }
                    }
                    Err(error) => {
                        this.mermaid_preview.error = Some(error.into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn release_mermaid_preview_images_after_frame(&mut self, window: &mut Window) {
        if self.mermaid_preview.retired_images.is_empty() {
            return;
        }
        let images = std::mem::take(&mut self.mermaid_preview.retired_images);
        window.on_next_frame(move |window, cx| {
            for image in images {
                cx.drop_image(image, Some(window));
            }
        });
    }

    pub(super) fn render_mermaid_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let source = self.mermaid_preview.source.clone();
        let body = if let Some(raster) = self.mermaid_preview.raster.clone() {
            div()
                .debug_selector(|| "workflow-mermaid-diagram".into())
                .w_full()
                .h_full()
                .min_w(px(raster.width + 48.))
                .min_h(px(raster.height + 48.))
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    img(ImageSource::Render(raster.image))
                        .w(px(raster.width))
                        .h(px(raster.height)),
                )
                .into_any_element()
        } else if let Some(error) = self.mermaid_preview.error.clone() {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .p_6()
                .text_color(theme.danger)
                .child("Could not render this workflow")
                .child(div().text_sm().child(error))
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Rendering diagram…")
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .p_6()
            .gap_3()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_3()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Rendered workflow"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("Review the generated Mermaid diagram before sharing or documenting the rule."),
                            ),
                    )
                    .child(
                        Button::new("workflow-copy-mermaid-source")
                            .label("Copy source")
                            .ghost()
                            .small()
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(source.to_string()))
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border.opacity(0.72))
                    .bg(theme.muted.opacity(0.12))
                    .overflow_hidden()
                    .child(
                        div()
                            .id("workflow-mermaid-preview-scroll")
                            .size_full()
                            .overflow_scrollbar()
                            .child(body),
                    ),
            )
            .into_any_element()
    }
}
