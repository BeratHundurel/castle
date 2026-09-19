---
name: gpui
description: Build and maintain Castle's GPUI Kit UI, including component state, themes, overlays, actions, async tasks, entities, focus, layout, custom elements, and tests. Use for GPUI framework work and dependency migrations; use design for visual direction.
---

## Castle Conventions

- Prefer GPUI Kit semantic controls and shared `Action`s; one command implementation serves toolbar, menu, context menu, and shortcut.
- Keep `render` declarative and side-effect-free: stable domain `ElementId`s, no per-frame retained entities/subscriptions/focus handles, `RenderOnce` for values and `Entity<T>` only for persistent behavior.
- Use `cx.theme()` plus rem helpers; reserve raw colors, radii, and `px(...)` for token definitions, measured geometry, or platform boundaries. Make focus, keyboard, disabled, overlay, and accessibility explicit.
- Model async as pending/success/failure, discard stale revisions, and never block the foreground executor: run SeaORM/SQLx on Tokio, apply results on the foreground executor.
- Test the lowest proving layer (pure state/geometry, then context, then `VisualTestContext`), covering pointer and keyboard paths, identity, focus, disabled, and empty/loading/failure states.

## Navigation

Load the relevant reference file based on the task:

| Topic                           | File                                                                    | When to load                                                                                        |
| ------------------------------- | ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Capability & crate organization | [architecture.md](references/architecture.md)                           | new capability or crate, moving feature code, cross-feature communication, ownership review         |
| Actions & keybindings           | [action.md](references/action.md)                                       | `actions!`, `bind_keys`, `on_action`, `key_context`                                                 |
| Async & background tasks        | [async.md](references/async.md)                                         | `cx.spawn`, `background_spawn`, `Task`, async I/O                                                   |
| Context management              | [context.md](references/context.md)                                     | `App`, `Window`, `Context<T>`, `AsyncApp`                                                           |
| Custom elements (low-level)     | [element.md](references/element.md)                                     | `Element` trait, `request_layout`, `prepaint`, `paint`                                              |
| Entity state                    | [entity.md](references/entity.md)                                       | `Entity<T>`, `WeakEntity`, state management                                                         |
| Events & subscriptions          | [event.md](references/event.md)                                         | `cx.emit`, `cx.subscribe`, `cx.observe`                                                             |
| Focus & keyboard nav            | [focus-handle.md](references/focus-handle.md)                           | `FocusHandle`, `track_focus`, Tab navigation                                                        |
| Global state                    | [global.md](references/global.md)                                       | `Global` trait, `cx.set_global`, app-wide config                                                    |
| Layout & styling                | [layout-style.md](references/layout-style.md)                           | `div()`, `h_flex()`, `v_flex()`, flexbox, overflow, positioning                                     |
| Layout, measurement & scrolling | [layout-measurement-scroll.md](references/layout-measurement-scroll.md) | Geometry-dependent behavior, prepaint bounds, alignment, overlays, scroll ownership                 |
| Performance & failure modes     | [performance.md](references/performance.md)                             | Render hot paths, notification ownership, retained state, virtualization, caching, closure captures |
| ElementId                       | [element-id.md](references/element-id.md)                               | `ElementId`, `.id()`, uniqueness rules, stateful elements                                           |
| Testing                         | [test.md](references/test.md)                                           | `#[gpui_kit::test]`, `TestAppContext`, `VisualTestContext`                                          |

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
