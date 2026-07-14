---
name: redesign-existing-projects
description: Audit and upgrade existing native Rust application surfaces built with GPUI and GPUI Components. Use when redesigning Castle views while preserving behavior, persistence, shortcuts, focus, themes, and established component patterns; web frameworks and browser concerns do not apply.
---

# Redesign Existing Native GPUI Surfaces

Improve the existing application in place. Preserve working behavior and user muscle memory while removing visual debt, interaction gaps, and generic UI patterns. Do not migrate stacks or reinterpret the task as a website redesign.

Read the `gpui` skill and relevant references. Treat `/docs` as read-only GPUI Components reference.

## 1. Audit before editing

Inspect the actual surface and record:

- Render tree and component boundaries.
- Entity state, events, subscriptions, actions, and key contexts.
- Focus handles, Tab order, default focus, and focus restoration.
- Theme tokens, typography, spacing, radii, borders, and icon use.
- Window-size behavior, overflow, scrolling, and resizable regions.
- Loading, empty, populated, error, disabled, dirty, saving, and saved states.
- Pointer, hover, pressed, selection, drag, drop-target, and cancel behavior.
- Persistence and async boundaries, especially database work.
- Existing GPUI Components that can replace bespoke implementations.
- Tests and user-visible behaviors that constrain the redesign.

Classify the work:

- **Targeted polish:** hierarchy, spacing, color, state visibility, or component consistency is weak but structure works.
- **Interaction repair:** focus, keyboard use, dragging, selection, errors, or async feedback is incomplete.
- **Structural redesign:** workflow or composition prevents the user from completing the primary task efficiently.

Do not choose structural redesign only because the current surface looks plain.

## 2. Diagnose native design problems

### Hierarchy and typography

- View title competes with content or consumes too much workspace.
- Metadata is as prominent as primary content.
- Every label uses the same size and weight.
- Counts or dates do not align for scanning.
- Text truncates without a way to reveal the full value.
- Long instructional copy appears inside frequent workflows.

### Color and surfaces

- Hardcoded RGB values bypass semantic theme tokens.
- Warm and cool neutrals mix without intent.
- Selection, hover, focus, and drop target look too similar.
- Danger or warning color is used decoratively.
- Every group has a border, background, radius, and shadow.
- Light and dark themes express different hierarchy or lose contrast.

### Layout

- Fixed dimensions fail in narrow or maximized windows.
- Toolbars contain infrequent actions and crowd the primary task.
- Everything is centered or split into equal panels despite unequal importance.
- Repeated card grids replace clearer rows, columns, or grouped regions.
- Scroll ownership is ambiguous or nested scrolling traps input.
- Wide views stretch editor text or card content beyond readable measure.
- Narrow views overlap controls or silently hide important actions.

### Interaction and states

- Clickable elements have no hover or pressed feedback.
- Keyboard focus is absent, low contrast, or in the wrong order.
- Essential actions appear only on hover.
- Selection disappears during async refresh.
- Loading feedback changes layout size or blocks unrelated work.
- Empty and error states do not offer the next useful action.
- Destructive actions are easy to trigger and hard to recover from.
- Drag-and-drop lacks source lift, valid target, invalid target, cancel, or failure feedback.
- Dialog focus is not contained or restored.

### Native behavior and code quality

- Existing GPUI Components are reimplemented inconsistently.
- Stateful elements use unstable or duplicate IDs.
- Event propagation causes nested controls to activate together.
- High-frequency pointer state rebuilds large render subtrees.
- Subscriptions have the wrong lifetime or form event loops.
- APIs are guessed instead of verified in code or `/docs`.
- SeaORM or SQLx work is awaited directly in `cx.spawn` or `cx.spawn_in`.
- SQLx futures are moved onto GPUI's background executor.

## 3. Preserve contracts

Do not change these silently during a redesign:

- Persistence format, database semantics, or save guarantees.
- Action names, keyboard shortcuts, and key contexts.
- Focus order, default focus, or focus restoration.
- Tab, selection, drag, and drop behavior.
- Command names and discoverability.
- Destructive-action confirmation or undo guarantees.
- Theme selection and user appearance settings.
- User-visible terminology and information architecture.

If a contract must change to solve the problem, call it out explicitly before implementation.

## 4. Upgrade priorities

Apply the smallest set that produces a coherent result:

1. Fix broken or missing interaction states.
2. Restore keyboard, focus, and action integrity.
3. Re-establish layout behavior across window sizes.
4. Consolidate colors into semantic theme tokens.
5. Clarify typography and content hierarchy.
6. Normalize spacing, radii, icons, and component variants.
7. Simplify toolbars, cards, borders, and decorative elements.
8. Add purposeful motion only where it explains change.
9. Replace a component or region only when the existing structure cannot support the workflow.

## 5. Native redesign techniques

### Typography

- Reduce oversized titles and give working content the space.
- Use medium or semibold weights for hierarchy before increasing size.
- Align numeric metadata and shortcuts.
- Shorten labels and helper text instead of shrinking them excessively.

### Layout

- Use `h_flex()` and `v_flex()` with explicit grow, shrink, min, and max behavior.
- Let the primary work region grow; keep tool and metadata regions constrained.
- Use controlled overflow and scroll ownership.
- Collapse secondary labels or move infrequent actions to overflow in narrow windows.
- Use a readable maximum width for editor content in wide windows.

### Surfaces

- Replace nested cards with grouping, spacing, and a single divider.
- Reserve elevation for transient layers such as menus, palettes, and dialogs.
- Use the established theme radius and border tokens.
- Make selected and focused states stronger than hover.

### Feedback

- Keep dirty, saving, saved, missing, and failed states stable in size and location.
- Use skeletons shaped like final content when loading is perceptible.
- Keep errors near their source and provide retry or recovery.
- Prefer undo for reversible deletion.
- Make valid drop targets obvious before release.

## 6. Implementation workflow

1. Read the relevant files and trace the current state and event flow.
2. Identify unrelated user changes in the worktree and preserve them.
3. Write a concise design diagnosis and intended contract-preserving changes.
4. Reuse existing theme tokens and GPUI Components.
5. Implement focused changes without broad rewrites.
6. Exercise every affected state and input method.
7. Verify narrow, typical, and wide window behavior and every supported theme.
8. Run focused tests, then `cargo check`.
9. Run `cargo clippy --fix --allow-dirty` for applicable code and review its edits.

## 7. Pre-flight

- [ ] The primary workflow is faster or clearer.
- [ ] Existing behavior and persistence contracts are preserved.
- [ ] Mouse, keyboard, focus, selection, and drag behavior work together.
- [ ] Loading, empty, error, disabled, and success states are complete.
- [ ] Layout works across realistic desktop window widths and display scales.
- [ ] Theme tokens replace arbitrary component-local colors.
- [ ] Typography, spacing, radii, icons, and surfaces are consistent.
- [ ] Motion is purposeful, short, and interruptible.
- [ ] Database work follows the Tokio-to-GPUI handoff rule.
- [ ] No user changes outside the task were overwritten.
- [ ] No HTML, CSS, React, Tailwind, browser, SEO, or web-performance guidance remains.
- [ ] Relevant tests and Rust checks pass.
