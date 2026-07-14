---
name: design-taste-frontend
description: Design and implement distinctive native desktop interfaces for Castle using Rust, GPUI, and GPUI Components. Use for new application surfaces, interaction design, visual systems, and polished native UI work where browser, HTML, CSS, React, Tailwind, and web-page conventions do not apply.
---

# Anti-Slop Native Application Design

Design Castle as a native productivity application. Treat the window, focus system, keyboard model, persistent data, and application state as parts of the design. Do not apply landing-page, browser, DOM, responsive-web, SEO, or scroll-storytelling advice.

Before editing GPUI code, read the `gpui` skill and the references relevant to the task. Use `/docs` as read-only reference for GPUI Components. Inspect existing application patterns before inventing an API or component.

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

## 2. Set three design dials

Use these values to reason about the surface. Do not expose them as user settings unless requested.

- `DESIGN_VARIANCE`: 1 is strictly conventional; 10 is highly expressive.
- `MOTION_INTENSITY`: 1 is nearly static; 10 is choreographed and physical.
- `VISUAL_DENSITY`: 1 is sparse; 10 is information-dense.

Castle defaults:

| Surface | Variance | Motion | Density |
|---|---:|---:|---:|
| Note editor | 4 | 3 | 5 |
| Kanban board | 5 | 5 | 7 |
| Home or overview | 6 | 4 | 5 |
| Command palette | 3 | 4 | 8 |
| Settings | 3 | 2 | 6 |
| Empty or onboarding state | 7 | 4 | 3 |

Raise variance for a meaningful product identity, not decoration. Raise density when comparison and scanning matter. Raise motion only when it clarifies feedback, continuity, or spatial change.

## 3. Use the native foundation

- Build with Rust, GPUI, and the existing GPUI Components dependency.
- Prefer existing project components and variants before creating a new component.
- Prefer `h_flex()` and `v_flex()` for normal composition.
- Use GPUI `Styled` methods and project theme tokens instead of hardcoded one-off values.
- Use `px()`, `rems()`, `relative()`, min/max constraints, flex growth, shrink rules, and overflow deliberately.
- Adapt composition from actual window bounds and available space. Do not copy browser breakpoint tables.
- Use render order, parent-child composition, and absolute positioning for stacking. Do not assume a general CSS-like `z-index` API exists.
- Verify every imported component or method in the repository, `/docs`, or the GPUI skill references. Do not hallucinate APIs.
- Keep application state in entities and update through the appropriate context. Call `cx.notify()` when a state change requires rendering.

## 4. Build a coherent visual system

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

## 5. Compose native application surfaces

### Window shell and navigation

- Keep the primary navigation model stable across tabs and views.
- Make active location, active tab, dirty state, and save state immediately legible.
- Preserve content space. Toolbars should contain frequent actions, not every possible command.
- Put infrequent actions in menus, command palette entries, or contextual controls.
- Ensure narrow windows degrade intentionally: collapse secondary labels, allow controlled scrolling, or move actions into overflow. Never let controls silently overlap or disappear.

### Kanban board

- Make column identity, card hierarchy, and drag targets distinct at a glance.
- Treat dragging as a complete state machine: idle, picked up, valid target, invalid target, autoscroll edge, committed, and cancelled.
- Keep card metadata subordinate to the task title. Show only fields useful for board-level decisions.
- Preserve usable column width and deliberate horizontal scrolling in narrow windows.
- Provide equivalent keyboard actions for essential card movement when feasible.

### Note editor

- Give content the visual priority. Keep editor chrome quiet until needed.
- Make title, body, metadata, and save state clearly distinct without excessive boxes.
- Preserve selection, cursor, and focus visibility in every theme.
- Avoid layout shifts while saving, loading attachments, or showing validation.
- Make destructive or irreversible actions explicit and recoverable where possible.

### Command palette and search

- Optimize for keyboard-first operation, fast scanning, and stable result rows.
- Keep query focus reliable when the palette opens.
- Distinguish selected, hovered, disabled, and unavailable results.
- Show shortcuts consistently and align them as metadata, not primary content.
- Preserve selection while asynchronous results update when the underlying item still exists.

### Settings and dialogs

- Group settings by user intent, not internal subsystem.
- Keep labels close to their controls and explain only non-obvious consequences.
- Use dialogs for focused decisions, confirmations, and short tasks. Prefer inline editing or side panels for ongoing work.
- Focus the first safe control on open, trap focus when appropriate, and restore focus on close.
- Default destructive confirmations to the safe action.

## 6. Design every interaction state

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

## 7. Use motion with restraint

Native motion must communicate one of four things: feedback, spatial continuity, hierarchy, or state change.

- Prefer short transitions for hover, selection, expansion, reordering, and panel appearance.
- Use spring-like movement only for direct manipulation such as drag-and-drop or reorder feedback.
- Avoid perpetual ambient animation in daily productivity surfaces.
- Keep progress indicators active only while work is actually pending.
- Respect reduced-motion preferences when the platform or current component APIs expose them.
- Use only animation APIs present in the existing GPUI stack. Do not add GSAP, web animation libraries, or browser event concepts.
- Avoid animation that requires expensive relayout on every frame. Preserve input responsiveness first.

## 8. Accessibility and native behavior

- Ensure complete mouse and keyboard operation for core workflows.
- Make Tab order follow visual and task order.
- Use `FocusHandle`, `.track_focus()`, `.key_context()`, actions, and key bindings consistently.
- Provide visible focus indicators with sufficient contrast.
- Use comfortable hit targets for pointer actions, especially compact icon buttons.
- Do not rely on hover-only labels for essential actions.
- Maintain readable contrast and semantic status differences in every supported theme.
- Verify text and controls at different font sizes, display scales, and window sizes.
- Use native text and components where possible so selection, input, and platform behavior remain reliable.

## 9. Protect performance and entity safety

- Keep render methods deterministic and cheap. Compute reusable or expensive data outside hot render paths.
- Use virtualized or scroll-aware components for large note, search, or board collections.
- Avoid rebuilding unrelated subtrees for high-frequency pointer or drag updates.
- Keep subscriptions alive for the intended lifetime and avoid event loops.
- Never await SeaORM or SQLx work directly inside `cx.spawn` or `cx.spawn_in`.
- Run database work through the active Tokio runtime, then apply the result to GPUI entities on the foreground executor.
- Do not move SQLx futures to GPUI's background executor.

## 10. Redesign workflow

When changing an existing surface:

1. Inspect the current render tree, state model, actions, focus handling, theme use, and component dependencies.
2. Capture the user workflow and states that must not regress.
3. Identify the smallest visual and interaction changes that solve the design problem.
4. Reuse existing components and tokens before adding new primitives.
5. Implement the complete state cycle, not only the ideal screenshot state.
6. Verify behavior at narrow, typical, and wide window sizes in light and dark themes where supported.
7. Run focused tests, then `cargo check` and `cargo clippy --fix --allow-dirty` when applicable.

Do not silently change persistence behavior, shortcuts, focus order, drag semantics, command names, or destructive-action guarantees as part of a visual redesign.

## 11. Native anti-patterns

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
- Web concepts such as DOM elements, HTML semantics, CSS selectors, media queries, browser breakpoints, viewport units, Tailwind classes, React hooks, SEO, or web performance metrics.

## 12. Pre-flight check

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
