# Castle

Castle is a Rust note-taking and Kanban app built with GPUI and GPUI Component.

## Architecture and Scope

- Keep a capability's state, views, commands, dialogs, and focused tests together. Extract a crate only for an independently owned capability with a stable public seam; keep dependencies acyclic and never make a feature depend on the app shell or sibling internals.
- Read the closest implementation, tests, public re-export seam, and Cargo-locked API source before editing. Current source and compiler output outrank examples or documentation from another revision.

## Tools

Use 'cargo clippy --fix --allow-dirty' for related files when it is applicable to ensure code quality

## Code Style

- Don't use unwrap.
- Don't comment obvious logic.
- Use precise domain names and established GPUI terminology. Name render helpers after meaningful regions and split a module when unrelated state ownership or lifecycle obscures it.

## Code Quality

- Avoid workarounds or hacks. Instead, find a better solution or implement the feature properly.
- Prefer GPUI Component's semantic controls and Actions to custom clickable `div`s or duplicated command mutations. One desktop command should share its implementation across toolbar, menu, context menu, and shortcut.
- Use stable domain-based `ElementId`s for repeated or retained UI. Never use labels, mutable list indices, or IDs generated during `render` when items can move.
- Keep `render` declarative and side-effect-free: compose from current state, move domain work to named methods/services, and never recreate retained entities, subscriptions, focus handles, or expensive data per frame.
- Choose `RenderOnce` for value-like presentation and an `Entity<T>` only when behavior must persist across frames. Keep state in its narrowest correct owner; callbacks request changes rather than creating a second source of truth. Notify once after each coherent rendering change, never unconditionally from `render`.

## Native UI Conventions

- Use semantic values from `cx.theme()` and rem-scale helpers (`gap_2`, `p_2`, `text_sm`, etc.). Do not add raw colors, radii, or `px(...)` layout nudges except for an intentional physical/platform boundary, measured runtime geometry, or token definition.
- Make keyboard, focus, disabled, overlay, and accessibility behavior explicit. Retain focus handles, attach actions and key contexts to the same focused region, and use the window `Root` rather than nesting roots inside a page.
- Give each scrollable panel one owner. Apply `min_w_0()`/`min_h_0()` to shrinkable flex children, avoid accidental nested scrolling, and put content insets inside the scroll owner.
- Model asynchronous work with explicit loading, success, and failure states; retain usable prior data while refreshing where appropriate. Identify requests/revisions and discard stale results.

## Async and Entity Safety

- Never do any blocking work on the foreground executor. Instead, offload it to the background executor or use a separate Tokio runtime.
- Never await SeaORM or SQLx work directly inside `cx.spawn` or `cx.spawn_in`; those tasks run on GPUI's foreground executor. Spawn database work through the current Tokio runtime, then apply the completed result to GPUI entities on the foreground executor.
- Don't move SQLx futures to GPUI's background executor. SQLx requires a Tokio runtime context.

## Verification

- Test the lowest layer that proves the behavior: pure state/geometry tests first, then GPUI context tests, then `VisualTestContext` interaction/layout tests. Cover pointer and keyboard paths, stable identity, focus, disabled behavior, and empty/loading/failure states when relevant.
- Add a deterministic regression test before fixing a reproducible bug. Report automated checks and manual visual acceptance separately.
- Run focused formatting, tests, and `cargo clippy --fix --allow-dirty` for related files when applicable; avoid overlapping Cargo jobs and use realistic timeouts.
