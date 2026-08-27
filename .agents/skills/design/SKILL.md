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

- Prefer existing project components and variants before creating a new component.
- Adapt composition from actual window bounds and available space. Do not copy browser breakpoint tables.

## 3. Build a coherent visual system

### Typography

- Establish a compact hierarchy for title, section heading, body, metadata, and shortcut text.
- Use the configured native font stack unless the project already bundles an intentional alternative.
- Reserve the largest type for rare moments. Productivity surfaces should prioritize scan speed and usable space.
- Keep body copy readable and short. Avoid long explanatory paragraphs inside working views.
- Use tabular or monospaced numerals when alignment helps compare counts, dates, or durations.
- Prevent clipping and truncation. Apply `.truncate()` only when the full value remains discoverable through selection, expansion, or another clear path.

### Color and themes

- Work within the existing semantic roles for background, surface, foreground, muted foreground, border, primary, danger, warning, success, info, hover, and drop target.
- Preserve semantic meaning across themes. Danger, selection, focus, and drop-target states must remain distinct.
- Use one restrained accent system. Do not introduce arbitrary colors for visual variety.
- Avoid pure black/white contrast when existing theme tokens provide more comfortable values.
- Test light and dark themes when both are supported.
- Never encode state by color alone. Pair color with iconography, copy, shape, or position.

### Spacing and shape

- Start from the project spacing rhythm, normally the GPUI shorthand scale where one step is 4 px.
- Use tighter spacing inside controls, moderate spacing between related groups, and generous spacing only between major regions.
- Pick a radius rule from the established system and apply it consistently.
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
- Stop event propagation intentionally where nested interactive regions would otherwise conflict.
- Keep errors contextual and actionable. Avoid vague alerts and silent failures.
- Make empty states explain the next useful action without marketing copy.
- Match skeleton/loading geometry to the final layout when a delay is perceptible.

## 6. Use motion with restraint

Native motion must communicate one of four things: feedback, spatial continuity, hierarchy, or state change.

- Prefer short, interruptible transitions for hover, selection, expansion, reordering, and panel appearance. Never make users wait for animation.
- Use spring-like movement only for direct manipulation such as drag-and-drop or reorder feedback.
- Avoid perpetual ambient animation, ornamental parallax, cursor trails, and scroll hijacking in daily productivity surfaces.
- Keep progress indicators active only while work is actually pending.
- Respect reduced-motion preferences when the platform or current component APIs expose them.
- Use only animation APIs present in the existing GPUI stack. Do not add GSAP, web animation libraries, or browser event concepts.
- Avoid animation that requires expensive relayout on every frame. Preserve input responsiveness first.

## 7. Accessibility and native behavior

- Ensure complete mouse and keyboard operation for core workflows.
- Make Tab order follow visual and task order.
- Provide visible focus indicators with sufficient contrast.
- Use comfortable hit targets for pointer actions, especially compact icon buttons.
- Do not rely on hover-only labels for essential actions.
- Maintain readable contrast and semantic status differences in every supported theme.
- Verify text and controls at different font sizes, display scales, and window sizes.
- Use native text and components where possible so selection, input, and platform behavior remain reliable.

## 8. Redesign workflow

When changing an existing surface:

1. Inspect the current render tree, state model, actions, focus handling, theme use, and component dependencies.
2. Capture the user workflow and states that must not regress.
3. Identify the smallest visual and interaction changes that solve the design problem.
4. Implement the complete state cycle, not only the ideal screenshot state.
5. Verify behavior across the window configurations identified in the design read.

Do not silently change persistence behavior, shortcuts, focus order, drag semantics, command names, or destructive-action guarantees as part of a visual redesign.

## 9. Native anti-patterns

Avoid these defaults unless the product context justifies them:

- A sidebar, top bar, and card grid copied from a generic SaaS dashboard.
- Excessive pills, badges, gradients, glass panels, and decorative status dots.
- Three identical summary cards as the automatic first layout.
- Huge headings that waste workspace area.
- Icon-only actions with unclear meaning.
- Inconsistent radii, icon weights, neutral palettes, or spacing scales.
- Fake precision, placeholder people, generic company names, or promotional copy in application data.
