---
name: minimalist-ui
description: Design quiet, editorial, utilitarian native desktop interfaces for Castle with GPUI and GPUI Components. Use for focused note, board, settings, search, and navigation surfaces that need warm monochrome themes, strong typography, restrained accents, and minimal visual noise.
---

# Premium Utilitarian Native UI

Build calm, precise native application surfaces. Minimalism means fewer competing signals and clearer work, not missing states or empty decoration. Apply this skill through Rust, GPUI, project theme tokens, and GPUI Components. Do not translate the rules into HTML, CSS, Tailwind, DOM, browser, or marketing-page patterns.

Read the `gpui` skill and relevant references before implementing unfamiliar framework behavior.

## 1. Visual character

- Use warm monochrome or softly neutral surfaces.
- Establish hierarchy through typography, spacing, and tone before borders or shadows.
- Keep the content canvas quiet and reserve accent color for selection, focus, primary action, or semantic status.
- Use flat, well-aligned groups. Add containment only when it communicates a real boundary.
- Prefer subtle contrast between background, surface, hover, and selected states.
- Avoid gradients, neon, glassmorphism, strong drop shadows, glossy effects, and decorative noise.

## 2. Theme tokens

Use `cx.theme()` semantic colors rather than hardcoded values whenever possible:

- `background` for the window canvas.
- `surface` or established secondary tokens for grouped regions.
- `foreground` for primary text.
- `muted_foreground` for secondary text that remains readable.
- `border` for structural separation.
- `primary` and `primary_foreground` for the main action or selection.
- `danger`, `warning`, `success`, and `info` only for their meanings.
- Existing hover, focus, and drop-target tokens for interaction feedback.

Do not introduce a new palette inside one component. Verify hierarchy and semantic states in every supported theme.

## 3. Typography

- Use the configured native font family unless an existing bundled font is part of the product identity.
- Use a compact scale: display or view title, section title, body, metadata, shortcut.
- Keep view titles strong but space-efficient.
- Use medium or semibold weight for hierarchy before increasing size.
- Keep labels in sentence case. Avoid uppercase tracking as a repeated decoration.
- Use monospaced or tabular numerals for shortcuts, counts, and aligned data when useful.
- Keep body text comfortable and concise; avoid dense help prose inside working views.
- Do not use placeholder copy, marketing clichés, or decorative metadata.

## 4. Spacing and geometry

- Follow the project's 4 px-based shorthand rhythm where possible.
- Use compact spacing within controls, moderate spacing inside groups, and larger spacing between regions.
- Align text baselines, icons, counts, and controls optically, not only mathematically.
- Use `cx.theme().radius` or one documented radius scale.
- Avoid pill shapes for large containers and primary controls. Reserve pills for compact tags or statuses whose shape carries meaning.
- Prefer one divider between groups to a border around every row.
- Use shadows only for transient elevation such as a menu, palette, or dialog, and keep them subdued.

## 5. Native components

### Buttons

- Use one clear primary action per focused region.
- Keep labels short and on one line.
- Provide hover, pressed, focus, and disabled feedback.
- Use icon-only buttons only when the meaning is conventional or a label is otherwise discoverable.

### Inputs

- Keep persistent labels near non-obvious fields.
- Do not rely on placeholder text as the only label.
- Make focus, validation, disabled, and read-only states distinct.
- Put error copy near the field and state how to recover.

### Lists and rows

- Prefer rows for scan-heavy notes, commands, settings, and history.
- Align leading icons, primary text, metadata, and trailing actions consistently.
- Keep selection stronger than hover and focus visible within both.
- Avoid putting each row in an individual card.

### Cards

- Use cards for kanban items, attachments, or bounded objects that move or select as a unit.
- Keep metadata subordinate and remove fields that do not help the current decision.
- Use a restrained border or surface shift; avoid border-plus-shadow-plus-tint all at once.

### Dialogs and palettes

- Use a clear title, focused content, and predictable action order.
- Focus the first safe control and restore previous focus on close.
- Keep command palettes dense, keyboard-first, and visually stable as results update.

## 6. Motion

- Use motion sparingly for selection, expansion, reorder, drag feedback, and transient surface appearance.
- Keep transitions short, interruptible, and subordinate to input.
- Avoid ambient loops and decorative motion in note or board workspaces.
- Respect reduced-motion behavior when supported.
- Use only verified GPUI APIs already available to the project.

## 7. Accessibility and resilience

- Provide complete keyboard access using focus handles, actions, key contexts, and bindings.
- Make focus indicators visible against all surfaces.
- Never communicate status by color alone.
- Keep pointer targets comfortable even when the visual icon is small.
- Verify narrow, typical, and wide window layouts and scaled text.
- Handle loading, empty, error, disabled, dirty, saving, and saved states without layout jumps.

## 8. Banned defaults

- A card around every group.
- Large gradients, glows, glass panels, or saturated color blocks.
- Heavy shadows and excessive elevation.
- Repeated uppercase micro-labels.
- Oversized headings in productivity views.
- Pill-shaped primary buttons and containers everywhere.
- Emojis as icons.
- Placeholder people, companies, metrics, or Latin filler.
- Hover-only essential actions.
- Hardcoded colors that bypass semantic theme tokens.
- Web implementation language such as HTML tags, CSS properties, Tailwind classes, React hooks, viewport units, media queries, or browser events.

## 9. Execution

1. Inspect the existing surface, theme use, component variants, focus flow, and state model.
2. Establish the hierarchy and macro layout first.
3. Apply typography, spacing, and semantic color tokens.
4. Add only the containment needed for selection, grouping, or elevation.
5. Implement all relevant interaction and data states.
6. Verify keyboard use, focus, theme parity, and multiple window widths.
7. Run relevant tests, `cargo check`, and `cargo clippy --fix --allow-dirty` when applicable.
