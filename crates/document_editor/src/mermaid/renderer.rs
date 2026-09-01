use anyhow::{Result, anyhow};
use lol_html::{RewriteStrSettings, element, rewrite_str};
use merman::render::{HeadlessRenderer, HostThemeOutput, HostThemeProfile, HostThemeRoles};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static DIAGRAM_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MermaidTheme {
    pub font_family: String,
    pub background: String,
    pub surface: String,
    pub surface_alt: String,
    pub foreground: String,
    pub muted_foreground: String,
    pub border: String,
    pub primary: String,
    pub warning: String,
    pub danger: String,
    pub success: String,
    pub chart_palette: Vec<String>,
    pub accent_surfaces: Vec<String>,
}

impl MermaidTheme {
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash as _, Hasher as _};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.font_family.hash(&mut hasher);
        self.background.hash(&mut hasher);
        self.surface.hash(&mut hasher);
        self.surface_alt.hash(&mut hasher);
        self.foreground.hash(&mut hasher);
        self.muted_foreground.hash(&mut hasher);
        self.border.hash(&mut hasher);
        self.primary.hash(&mut hasher);
        self.warning.hash(&mut hasher);
        self.danger.hash(&mut hasher);
        self.success.hash(&mut hasher);
        self.chart_palette.hash(&mut hasher);
        self.accent_surfaces.hash(&mut hasher);
        hasher.finish()
    }

    pub fn prepare(&self) -> PreparedMermaidRenderer {
        let roles = HostThemeRoles {
            canvas: Some(self.background.clone()),
            surface: Some(self.surface.clone()),
            surface_alt: Some(self.surface_alt.clone()),
            surface_muted: Some(self.surface_alt.clone()),
            text: Some(self.foreground.clone()),
            subtle_text: Some(self.muted_foreground.clone()),
            border: Some(self.border.clone()),
            line: Some(self.muted_foreground.clone()),
            edge_label_background: Some(self.background.clone()),
            cluster_background: Some(self.surface_alt.clone()),
            cluster_border: Some(self.border.clone()),
            note_background: Some(self.surface_alt.clone()),
            note_border: Some(self.warning.clone()),
            note_text: Some(self.foreground.clone()),
            actor_background: Some(self.surface.clone()),
            actor_border: Some(self.border.clone()),
            actor_text: Some(self.foreground.clone()),
            activation_background: Some(self.surface_alt.clone()),
            activation_border: Some(self.primary.clone()),
            error: Some(self.danger.clone()),
            warning: Some(self.warning.clone()),
            success: Some(self.success.clone()),
        };
        let font_family = if self.font_family.to_ascii_lowercase().contains("sans-serif") {
            self.font_family.clone()
        } else {
            format!("{}, sans-serif", self.font_family)
        };
        let mut output = HostThemeOutput::resvg_safe_editor();
        output.scoped_css = Some(themed_shape_css(self));
        let profile = HostThemeProfile::builder()
            .font_family(font_family)
            .font_size("16px")
            .roles(roles)
            .series_palette(self.chart_palette.clone())
            .output(output)
            .build();
        PreparedMermaidRenderer {
            renderer: HeadlessRenderer::new()
                .with_compiled_host_theme(&profile.compile())
                .with_vendored_text_measurer(),
            accent_count: self.chart_palette.len().min(self.accent_surfaces.len()),
        }
    }
}

#[derive(Clone)]
pub struct PreparedMermaidRenderer {
    renderer: HeadlessRenderer,
    accent_count: usize,
}

impl PreparedMermaidRenderer {
    pub fn render_to_svg(&self, source: &str) -> Result<String> {
        if !is_supported_diagram(source) {
            return Err(anyhow!("unsupported Mermaid diagram type"));
        }

        let id = DIAGRAM_ID.fetch_add(1, Ordering::Relaxed);
        let diagram_id = format!("castle-mermaid-{id}");
        let svg = self
            .renderer
            .clone()
            .with_diagram_id(&diagram_id)
            .render_svg_sync(source)?
            .ok_or_else(|| anyhow!("Merman produced no SVG"))?;
        assign_accent_classes(&svg, self.accent_count)
    }
}

fn themed_shape_css(theme: &MermaidTheme) -> String {
    let mut css = format!(
        r#"
.node > rect,
.node > circle,
.node > ellipse,
.node > polygon,
.node > path,
.classGroup > rect,
.stateGroup > rect,
.mindmap-node > rect,
.mindmap-node > circle,
.mindmap-node > polygon {{ stroke-width: 1.5px; }}
.flowchart-link,
.messageLine0,
.messageLine1,
.transition {{ stroke: {}; stroke-width: 1.5px; }}
.nodeLabel,
.messageText,
.label text {{ fill: {}; font-weight: 500; }}
.edgeLabel rect {{ fill: {}; opacity: 0.96; }}
"#,
        theme.muted_foreground, theme.foreground, theme.background
    );

    for (index, (stroke, fill)) in theme
        .chart_palette
        .iter()
        .zip(&theme.accent_surfaces)
        .take(5)
        .enumerate()
    {
        css.push_str(&format!(
            r#"
.castle-mermaid-accent-{index} > rect,
.castle-mermaid-accent-{index} > circle,
.castle-mermaid-accent-{index} > ellipse,
.castle-mermaid-accent-{index} > polygon,
.castle-mermaid-accent-{index} > path,
rect.castle-mermaid-accent-{index} {{ fill: {fill}; stroke: {stroke}; }}
"#
        ));
    }
    css
}

fn append_class(
    element: &mut lol_html::html_content::Element<'_, '_>,
    class_name: &str,
) -> std::result::Result<(), lol_html::errors::AttributeNameError> {
    let classes = element.get_attribute("class").unwrap_or_default();
    let classes = if classes.is_empty() {
        class_name.to_string()
    } else {
        format!("{classes} {class_name}")
    };
    element.set_attribute("class", &classes)
}

fn assign_accent_classes(svg: &str, accent_count: usize) -> Result<String> {
    if accent_count == 0 {
        return Ok(svg.to_string());
    }

    let node_index = Rc::new(Cell::new(0usize));
    let node_index_for_handler = node_index.clone();
    let actor_positions = Rc::new(RefCell::new(HashMap::<String, usize>::new()));
    let actor_positions_for_handler = actor_positions.clone();
    let next_actor_index = Rc::new(Cell::new(0usize));
    let next_actor_index_for_handler = next_actor_index.clone();

    Ok(rewrite_str(
        svg,
        RewriteStrSettings {
            element_content_handlers: vec![
                element!(
                    "g.node, g.mindmap-node, g.classGroup, g.stateGroup",
                    move |element| {
                        let index = node_index_for_handler.get() % accent_count;
                        node_index_for_handler.set(node_index_for_handler.get() + 1);
                        append_class(element, &format!("castle-mermaid-accent-{index}"))?;
                        Ok(())
                    }
                ),
                element!("rect.actor", move |element| {
                    let position = element
                        .get_attribute("x")
                        .unwrap_or_else(|| next_actor_index_for_handler.get().to_string());
                    let index = *actor_positions_for_handler
                        .borrow_mut()
                        .entry(position)
                        .or_insert_with(|| {
                            let next = next_actor_index_for_handler.get();
                            next_actor_index_for_handler.set(next + 1);
                            next % accent_count
                        });
                    append_class(element, &format!("castle-mermaid-accent-{index}"))?;
                    Ok(())
                }),
            ],
            ..RewriteStrSettings::new()
        },
    )?)
}

pub fn is_supported_diagram(source: &str) -> bool {
    let Some(kind) = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .and_then(|line| line.split_whitespace().next())
    else {
        return false;
    };

    [
        "flowchart",
        "graph",
        "sequenceDiagram",
        "classDiagram",
        "stateDiagram",
        "stateDiagram-v2",
        "erDiagram",
        "gantt",
        "pie",
        "gitGraph",
        "mindmap",
        "timeline",
        "quadrantChart",
        "xychart-beta",
        "journey",
    ]
    .iter()
    .any(|supported| kind.eq_ignore_ascii_case(supported))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> MermaidTheme {
        MermaidTheme {
            font_family: "Inter".into(),
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
        }
    }

    #[test]
    fn recognizes_the_supported_allowlist() {
        for source in [
            "flowchart LR\nA-->B",
            "graph TD\nA-->B",
            "sequenceDiagram\nA->>B: Hello",
            "classDiagram\nA <|-- B",
            "stateDiagram-v2\n[*] --> Ready",
            "erDiagram\nA ||--o{ B : owns",
            "gantt\ntitle Work",
            "pie\n\"A\" : 1",
            "gitGraph\ncommit",
            "mindmap\n root((Castle))",
            "timeline\n2026 : Castle",
            "quadrantChart\nx-axis Low --> High",
            "xychart-beta\nx-axis [1, 2]",
            "journey\ntitle Work",
        ] {
            assert!(is_supported_diagram(source), "{source}");
        }
        assert!(!is_supported_diagram("sankey-beta\na,b,1"));
        assert!(!is_supported_diagram("%% comment only"));
    }

    #[test]
    fn fingerprint_tracks_rendering_tokens() {
        let first = theme();
        let mut second = first.clone();
        assert_eq!(first.fingerprint(), second.fingerprint());
        second.primary = "#ff0000".into();
        assert_ne!(first.fingerprint(), second.fingerprint());
        second = first.clone();
        second.accent_surfaces[0] = "#fef3c7".into();
        assert_ne!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn renders_resvg_safe_svg() {
        let svg = theme()
            .prepare()
            .render_to_svg("flowchart LR\nA[Castle] --> B[Preview]")
            .expect("representative Mermaid should render");
        assert!(svg.starts_with("<svg") || svg.contains("<svg"));
        assert!(!svg.contains("foreignObject"));
        assert!(svg.contains("#dbeafe"));
        assert!(svg.contains("castle-mermaid-accent-0"));
    }

    #[test]
    fn assigns_distinct_accents_to_nodes_and_repeated_actors() {
        let svg = assign_accent_classes(
            r#"<svg><g class="nodes"><g class="node"><rect/></g><g class="node"><path/></g></g><rect class="actor" x="10"/><rect class="actor" x="20"/><rect class="actor" x="10"/></svg>"#,
            3,
        )
        .expect("accent assignment should preserve valid SVG");
        assert!(svg.contains(r#"class="node castle-mermaid-accent-0""#));
        assert!(svg.contains(r#"class="node castle-mermaid-accent-1""#));
        assert_eq!(svg.matches("actor castle-mermaid-accent-0").count(), 2);
        assert_eq!(svg.matches("actor castle-mermaid-accent-1").count(), 1);
    }

    #[test]
    fn renders_representative_diagrams_in_light_and_dark_themes() {
        let samples = [
            ("flowchart", "flowchart LR\nA[Castle] --> B[Preview]"),
            ("sequence", "sequenceDiagram\nAlice->>Bob: Hello"),
            ("class", "classDiagram\nAnimal <|-- Duck"),
            ("er", "erDiagram\nCUSTOMER ||--o{ ORDER : places"),
            (
                "gantt",
                "gantt\ntitle Release\nsection Work\nRender :done, a1, 2026-01-01, 1d",
            ),
            ("pie", "pie title Notes\n\"Markdown\" : 70\n\"Boards\" : 30"),
            (
                "mindmap",
                "mindmap\n  root((Castle))\n    Notes\n    Boards",
            ),
            (
                "state",
                "stateDiagram-v2\n[*] --> Editing\nEditing --> Preview",
            ),
        ];
        let light = theme();
        let mut dark = theme();
        dark.background = "#111827".into();
        dark.surface = "#1f2937".into();
        dark.surface_alt = "#273449".into();
        dark.foreground = "#f8fafc".into();
        dark.muted_foreground = "#a8b3c7".into();
        dark.border = "#475569".into();
        dark.accent_surfaces = vec!["#172554".into(), "#2e1065".into(), "#083344".into()];

        for current_theme in [&light, &dark] {
            let renderer = current_theme.prepare();
            for (name, source) in samples {
                let svg = renderer
                    .render_to_svg(source)
                    .unwrap_or_else(|error| panic!("{name} failed: {error}"));
                assert!(svg.contains("<svg"), "{name}");
                assert!(!svg.contains("foreignObject"), "{name}");
            }
        }
    }
}
