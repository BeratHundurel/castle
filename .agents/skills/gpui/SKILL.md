---
name: gpui
description: GPUI framework knowledge covering actions/keybindings, async/background tasks, context management (App/Window/Context<T>/AsyncApp), custom elements (low-level Element trait), entity state management, event system, focus handling, global state, layout/styling (flexbox/CSS-like), and testing. Use when working with any GPUI framework concept, building GPUI applications, or needing guidance on GPUI-specific APIs and patterns.
---

## Castle Conventions

For application work, use GPUI Component's semantic controls and Actions rather than custom clickable `div`s or duplicate command mutations. A desktop command should share its implementation across toolbar, menu, context menu, and shortcut.

- Give repeated or retained interactive elements stable domain-based `ElementId`s. Never derive them from mutable labels, list indexes, or render-time generation.
- Keep `render` declarative and side-effect-free. Do not recreate retained entities, subscriptions, focus handles, or expensive data per frame. Use `RenderOnce` for value-like presentation and `Entity<T>` only when behavior must persist.
- Use `cx.theme()` and rem-scale helpers such as `gap_2`, `p_2`, and `text_sm`; use raw colors, radii, or `px(...)` only for an intentional physical/platform boundary, measured geometry, or token definition.
- Make focus, keyboard, disabled, overlay, and accessibility behavior explicit. Use one scroll owner per panel; allow shrinkable flex children with `min_w_0()`/`min_h_0()` and keep content insets inside that owner.
- Represent async work with loading, success, and failure states; retain usable data while refreshing and discard results whose request or revision is stale.
- Never block the foreground executor. Run SeaORM and SQLx work on the current Tokio runtime, not in `cx.spawn`, `cx.spawn_in`, or GPUI's background executor; apply completed results to entities on the foreground executor.

For UI behavior, test the lowest proving layer: pure state or geometry tests, then GPUI context tests, then `VisualTestContext` interaction or layout tests. Cover pointer and keyboard paths, stable identity, focus, disabled behavior, and relevant empty, loading, and failure states.

## Navigation

Load the relevant reference file based on the task:

| Topic | File | When to load |
|-------|------|--------------|
| Actions & keybindings | [action.md](references/action.md) | `actions!`, `bind_keys`, `on_action`, `key_context` |
| Async & background tasks | [async.md](references/async.md) | `cx.spawn`, `background_spawn`, `Task`, async I/O |
| Context management | [context.md](references/context.md) | `App`, `Window`, `Context<T>`, `AsyncApp` |
| Custom elements (low-level) | [element.md](references/element.md) | `Element` trait, `request_layout`, `prepaint`, `paint` |
| Entity state | [entity.md](references/entity.md) | `Entity<T>`, `WeakEntity`, state management |
| Events & subscriptions | [event.md](references/event.md) | `cx.emit`, `cx.subscribe`, `cx.observe` |
| Focus & keyboard nav | [focus-handle.md](references/focus-handle.md) | `FocusHandle`, `track_focus`, Tab navigation |
| Global state | [global.md](references/global.md) | `Global` trait, `cx.set_global`, app-wide config |
| Layout & styling | [layout-style.md](references/layout-style.md) | `div()`, `h_flex()`, `v_flex()`, flexbox, overflow, positioning |
| ElementId | [element-id.md](references/element-id.md) | `ElementId`, `.id()`, uniqueness rules, stateful elements |
| Testing | [test.md](references/test.md) | `#[gpui::test]`, `TestAppContext`, `VisualTestContext` |

## Extended References

For deep-dive topics, additional reference files are available:

**Element trait:**
- [element-api.md](references/element-api.md) — complete API, hitbox system, event handling
- [element-patterns.md](references/element-patterns.md) — text, interactive, container, composite patterns
- [element-examples.md](references/element-examples.md) — full examples: text, interactive, complex elements
- [element-best-practices.md](references/element-best-practices.md) — performance, state, common pitfalls
- [element-advanced.md](references/element-advanced.md) — masonry/circular layouts, async updates, virtual lists

**Entity management:**
- [entity-api.md](references/entity-api.md) — complete Entity API, methods, lifecycle
- [entity-patterns.md](references/entity-patterns.md) — model-view, cross-entity communication, observer
- [entity-best-practices.md](references/entity-best-practices.md) — memory, performance, lifecycle
- [entity-advanced.md](references/entity-advanced.md) — collections, registry, debounce, state machines

**Testing:**
- [test-examples.md](references/test-examples.md) — testing examples and patterns
- [test-reference.md](references/test-reference.md) — complete testing API reference
