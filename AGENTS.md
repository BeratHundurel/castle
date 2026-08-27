# Castle

Castle is a Rust note-taking and Kanban app built with GPUI and GPUI Component.

## Working Agreement

Before editing, an agent must read the nearest implementation, its tests, the
re-export seam, and the relevant component documentation. It must search the
current source for signatures instead of translating a React, CSS, or old GPUI
example by analogy. For GPUI work, it must load the `gpui` skill and the
references it routes to for the task.

## Common failure modes

Avoid these patterns:

- One entity containing the entire application's unrelated state. Split by behavior ownership and lifecycle rather than by arbitrary visual fragments.
- Business logic, persistence, or network requests embedded in a long `render` method. Rendering should describe presentation from already-owned state.
- Duplicated local state that can drift from a controlled model value. Keep one source of truth and derive presentation state unless the duplicate has an explicit synchronization and lifecycle contract.
- `cx.notify()` loops caused by mutation during every render, prepaint, observer callback, or mutually observing entity cycle.
- A new component variant for a one-off screen. Prefer composition or a local presentation exception unless the variant represents a reusable semantic contract.
- Confirmation dialogs for reversible, low-risk actions. Apply the action immediately and provide undo or another clear recovery path.

## Code Style

- Use precise domain names and established GPUI terminology. Name render helpers after meaningful regions and split a module when unrelated state ownership or lifecycle obscures it.
- Don't use unwrap.
- Don't comment obvious logic.

## Code Quality

- Avoid workarounds or hacks. Instead, find a better solution or implement the feature properly.

## Verification

- Add a deterministic regression test before fixing a reproducible bug. Report automated checks and manual visual acceptance separately.
