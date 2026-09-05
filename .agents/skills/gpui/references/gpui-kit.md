# GPUI Kit integration

## Dependencies and bootstrap

Castle uses the published `gpui-kit` 0.6 family. Read the [workspace manifest](../../../../Cargo.toml)
and [lockfile](../../../../Cargo.lock) for the exact resolved versions. Do not mix it with the old Zed
Git dependencies: Kit brings a matching `gpui-pre-*` runtime family.

- In member manifests, use `gpui-kit.workspace = true`.
- Import GPUI types, derives, and `actions!` from `gpui_kit`; import styled
  controls from `gpui_kit::component` and behavior from `gpui_kit::base`.
- Start with `gpui_kit::application().with_assets(gpui_kit::assets::Assets)`.
  Call `gpui_kit::init(cx)` before using components. Keep the existing window
  `Root` and overlay ownership.
- The app's HTTP adapter uses `gpui-pre-reqwest-client` directly because
  Kit 0.6 does not re-export it. Keep it on Kit's compatible runtime family.
- Enable `tree-sitter-languages` on the document editor's Kit dependency.
- Enable `test-support` on each UI crate's Kit dev-dependency and use
  `#[gpui_kit::test]`. Prefer explicit imports in test modules: glob imports
  also bring in the framework's `test` attribute, which can shadow `#[test]`.
- Cargo profile overrides name the actual packages: `gpui-pre` and
  `gpui-pre-platform`, not the old dependency names.

## Components and current signatures

Check the resolved source and its re-exports before editing. The release
notes are an overview; exact signatures can differ from their examples.

- Single-line fields use `Input` / `InputState`.
- Multiline prose uses `Textarea` / `TextareaState`.
- Source editing uses `Editor` / `EditorState`. In 0.6.0, construct it with
  `EditorState::new(window, cx).language("rust")`.
- `Table` is declarative; stateful tables use `DataTable`.
- Use `Separator`, `UndoHistory` for undo transactions, and `History` for
  back/forward navigation. Do not copy the removed `Divider` or `HistoryItem` APIs.
- Theme-aware popover styling comes from `ThemeStyled`.
- Public context/snapshot types may expose constructors, builders, and readers
  rather than fields. Verify the specific type before constructing it.

## Local implementation references

- [Application bootstrap](../../../../crates/app/src/main.rs): initialization,
  assets, HTTP client, fonts, themes, and window root.
- [Document editor](../../../../crates/document_editor/src/lib.rs): retained
  editor state and language configuration.
- [Quick capture](../../../../crates/quick_capture/src/view.rs): textarea state,
  submission, and focus.
- [Theme settings](../../../../crates/settings/src/store.rs): theme application
  and synchronization with the base layer.

For exact component documentation and signatures, run
`cargo metadata --locked --format-version 1` and find the `manifest_path` for
`gpui-kit`, `gpui-component`, or `gpui-base`. Read the source files beside that
manifest: the facade's `src/lib.rs`, the component's module and re-export seam,
and the underlying base implementation. These are the resolved release's local
files; do not substitute examples from a newer website version.

## Local guidance and maintenance

Use [Design](../../design/SKILL.md) for visual and interaction decisions,
[architecture](../../architecture/SKILL.md) for capability boundaries, and
[GPUI](../SKILL.md) for state, lifecycle, and testing references.

The shared GPUI references were refreshed from gpui-kit v0.6.0, commit
`94a313a72a2513aee2780240cd322d552b2395f0`. When syncing, preserve Castle's
[domain-ID correction](element-id.md),
[layout and scrolling guidance](layout-measurement-scroll.md), and
[performance guidance](performance.md). The
[working agreement](../../../../AGENTS.md) and explicit user instructions take
precedence over upstream examples.
