---
name: design
description: Design and implement distinctive native desktop interfaces for Castle using Rust, GPUI, and GPUI Components. Use for new application surfaces, interaction design, visual systems, and polished native UI work.
---

# Anti-Slop Native Application Design

Design Castle as a native productivity application. Treat the window, focus system, keyboard model, persistent data, and application state as parts of the design.

Before editing GPUI code, read the `gpui` skill and the references relevant to the task.

## 1. Read the product context

Infer and state one concise design read before implementation:

`Reading this as: <native surface> for <user task>, with <density> density, <interaction priority>, and <visual character>.`

Consider:

- Surface: note editor, kanban board, home, settings, command palette, dialog, sidebar, tab strip, or system feedback.
- Primary job: scanning, editing, organizing, navigating, searching, configuring, or recovering from failure.
- Input: mouse, keyboard, drag-and-drop, text entry, or a deliberate combination.
- Frequency: a repeated daily workflow should be quieter and faster than a rare onboarding or destructive flow.
- Window behavior: narrow, typical, and wide desktop windows; maximized and restored states; platform chrome and density.
- Existing product language: theme tokens, component variants, icons, spacing, copy, shortcuts, and state models.

Ask one clarifying question only when two plausible interpretations would produce materially different workflows. Otherwise proceed with the strongest inference.

## 2. Use the native foundation

- Build with Rust, GPUI, and the existing GPUI Components dependency.
- Prefer existing project components and variants before creating a new component.
- Prefer `h_flex()` and `v_flex()` for normal composition.
- Use GPUI `Styled` methods and project theme tokens instead of hardcoded one-off values.
- Adapt composition from actual window bounds and available space. Do not copy browser breakpoint tables.
- Use render order, parent-child composition, and absolute positioning for stacking. Do not assume a general CSS-like `z-index` API exists.
- Keep application state in entities and update through the appropriate context. Call `cx.notify()` when a state change requires rendering.

## 3. Build a coherent visual system

### Typography

- Establish a compact hierarchy for title, section heading, body, metadata, and shortcut text.
- Use the configured native font stack unless the project already bundles an intentional alternative.
- Reserve the largest type for rare moments. Productivity surfaces should prioritize scan speed and usable space.
- Keep body copy readable and short. Avoid long explanatory paragraphs inside working views.
- Use tabular or monospaced numerals when alignment helps compare counts, dates, or durations.
- Prevent clipping and truncation. Apply `.truncate()` only when the full value remains discoverable through selection, expansion, or another clear path.

### Color and themes

- Use semantic `cx.theme()` tokens such as background, surface, foreground, muted foreground, border, primary, danger, warning, success, info, hover, and drop target.
- Preserve semantic meaning across themes. Danger, selection, focus, and drop-target states must remain distinct.
- Use one restrained accent system. Do not introduce arbitrary colors for visual variety.
- Avoid pure black/white contrast when existing theme tokens provide more comfortable values.
- Test light and dark themes when both are supported.
- Never encode state by color alone. Pair color with iconography, copy, shape, or position.

### Spacing and shape

- Start from the project spacing rhythm, normally the GPUI shorthand scale where one step is 4 px.
- Use tighter spacing inside controls, moderate spacing between related groups, and generous spacing only between major regions.
- Pick a radius rule and apply it consistently. Use `cx.theme().radius` when it represents the established system.
- Do not turn every group into a card. Prefer spacing, alignment, a shared surface, or a single divider when elevation is not meaningful.
- Use shadows sparingly. Native productivity interfaces benefit more from surface contrast and borders than floating marketing-card treatment.

### Icons and imagery

- Use the existing GPUI Components icon set and keep size, weight, and optical alignment consistent.
- Do not add emojis as substitute icons.
- Do not draw decorative SVGs or introduce a second icon family without a concrete gap in the current set.
- Use illustrations or imagery only when they help onboarding, empty states, attachments, or content comprehension. Working surfaces do not require decorative hero art.

## 4. Compose native application surfaces

### Window shell and navigation

- Keep the primary navigation model stable across tabs and views.
- Make active location, active tab, dirty state, and save state immediately legible.
- Preserve content space. Toolbars should contain frequent actions, not every possible command.
- Put infrequent actions in menus, command palette entries, or contextual controls.
- Ensure narrow windows degrade intentionally: collapse secondary labels, allow controlled scrolling, or move actions into overflow. Never let controls silently overlap or disappear.

## 5. Design every interaction state

For each interactive component, cover the states that apply:

- Rest, hover, pressed, focused, selected, disabled.
- Loading, empty, populated, stale, error, retrying.
- Drag source, valid drop target, invalid drop target, drag cancelled.
- Dirty, saving, saved, conflict, missing file, save failed.

Requirements:

- Hover cannot be the only discoverability or feedback mechanism.
- Pressed feedback should be immediate and subtle.
- Keyboard focus must be clearly visible using `FocusHandle`, `.track_focus()`, and theme-aware focus styling.
- Register actions and key contexts for keyboard behavior rather than scattering raw key checks.
- Stop event propagation intentionally where nested interactive regions would otherwise conflict.
- Use stable, unique `ElementId` values for stateful or interactive elements.
- Keep errors contextual and actionable. Avoid vague alerts and silent failures.
- Make empty states explain the next useful action without marketing copy.
- Match skeleton/loading geometry to the final layout when a delay is perceptible.

## 6. Use motion with restraint

Native motion must communicate one of four things: feedback, spatial continuity, hierarchy, or state change.

- Prefer short transitions for hover, selection, expansion, reordering, and panel appearance.
- Use spring-like movement only for direct manipulation such as drag-and-drop or reorder feedback.
- Avoid perpetual ambient animation in daily productivity surfaces.
- Keep progress indicators active only while work is actually pending.
- Respect reduced-motion preferences when the platform or current component APIs expose them.
- Use only animation APIs present in the existing GPUI stack. Do not add GSAP, web animation libraries, or browser event concepts.
- Avoid animation that requires expensive relayout on every frame. Preserve input responsiveness first.

## 7. Accessibility and native behavior

- Ensure complete mouse and keyboard operation for core workflows.
- Make Tab order follow visual and task order.
- Use `FocusHandle`, `.track_focus()`, `.key_context()`, actions, and key bindings consistently.
- Provide visible focus indicators with sufficient contrast.
- Use comfortable hit targets for pointer actions, especially compact icon buttons.
- Do not rely on hover-only labels for essential actions.
- Maintain readable contrast and semantic status differences in every supported theme.
- Verify text and controls at different font sizes, display scales, and window sizes.
- Use native text and components where possible so selection, input, and platform behavior remain reliable.

## 8. Protect performance and entity safety

- Keep render methods deterministic and cheap. Compute reusable or expensive data outside hot render paths.
- Avoid rebuilding unrelated subtrees for high-frequency pointer or drag updates.
- Keep subscriptions alive for the intended lifetime and avoid event loops.

## 9. Redesign workflow

When changing an existing surface:

1. Inspect the current render tree, state model, actions, focus handling, theme use, and component dependencies.
2. Capture the user workflow and states that must not regress.
3. Identify the smallest visual and interaction changes that solve the design problem.
4. Reuse existing components and tokens before adding new primitives.
5. Implement the complete state cycle, not only the ideal screenshot state.
6. Verify behavior at narrow, typical, and wide window sizes in light and dark themes where supported.
7. Run focused tests, then `cargo check` and `cargo clippy --fix --allow-dirty` when applicable.

Do not silently change persistence behavior, shortcuts, focus order, drag semantics, command names, or destructive-action guarantees as part of a visual redesign.

## 10. Native anti-patterns

Avoid these defaults unless the product context justifies them:

- A sidebar, top bar, and card grid copied from a generic SaaS dashboard.
- A card around every text block.
- Excessive pills, badges, gradients, glass panels, and decorative status dots.
- Three identical summary cards as the automatic first layout.
- Huge headings that waste workspace area.
- Hover-only controls with no keyboard path.
- Icon-only actions with unclear meaning.
- Inconsistent radii, icon weights, neutral palettes, or spacing scales.
- Fake precision, placeholder people, generic company names, or promotional copy in application data.
- Animation added only to make the interface feel impressive.
- Raw RGB values where semantic theme tokens already exist.
- Fixed dimensions that fail when the window narrows or display scale changes.

## 11. Pre-flight check

Before finishing, verify:

- [ ] The surface and primary user job are clear.
- [ ] Existing GPUI Components and theme tokens were reused where appropriate.
- [ ] Layout behaves intentionally in narrow, typical, and wide windows.
- [ ] Typography, spacing, radii, icons, and color form one coherent system.
- [ ] Rest, hover, pressed, focus, selected, disabled, loading, empty, and error states are covered where relevant.
- [ ] Keyboard navigation and visible focus work.
- [ ] Drag-and-drop has clear source, target, cancel, and failure feedback where relevant.
- [ ] Copy is specific, concise, and free of placeholder or marketing language.
- [ ] Motion has a functional reason and does not compromise responsiveness.
- [ ] Database work follows the project's Tokio-to-GPUI handoff rule.
- [ ] No browser or web-stack assumptions leaked into the implementation.
- [ ] Relevant checks and tests pass.
