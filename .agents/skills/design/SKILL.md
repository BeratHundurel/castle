---
name: design
description: Design and refine Castle's native GPUI Kit interfaces, including layout, visual hierarchy, interaction states, accessibility, and purposeful motion. Use for new surfaces, redesigns, or UI quality reviews; use gpui for framework APIs and lifecycle behavior.
---

# Design

Build Castle around writing, scanning, and organizing. Give the primary task
most of the space and keep supporting controls quiet without hiding them.

Read the [GPUI skill](../gpui/SKILL.md) for implementation rules and
[layout and measurement](../gpui/references/layout-measurement-scroll.md)
when changing sizing, scrolling, or overlays. Use
[architecture](../architecture/SKILL.md) when the change affects feature ownership.

## Understand the surface

Inspect the existing view, commands, state ownership, theme, and nearby controls.
For a new surface or substantial redesign, briefly explain the primary task,
composition, and intended interaction before implementing. Small edits do not
need a separate design plan.

Preserve existing shortcuts, persistence behavior, focus order, drag semantics,
and recovery guarantees unless the requested change calls for changing them.

## Compose around the task

- Use a focused canvas for writing, a horizontal board for organizing, and a
  master-detail layout or inspector when users compare items and properties.
  Choose proportions by task importance rather than giving every region equal weight.
- Keep navigation and active location stable. Put frequent commands in the
  toolbar and less frequent commands in menus or the command palette.
- Use inline disclosure for brief edits, an inspector for persistent properties,
  and a contextual toolbar for selection actions. Keep a keyboard path to each command.
- Adapt to actual window bounds. At narrow widths, collapse secondary labels,
  provide deliberate scrolling, or move commands into overflow.
- Prefer existing GPUI Kit controls and composition. Do not turn every group
  into a card or add a new variant for a single screen.

## Keep the visual system coherent

- Use Castle's configured fonts, semantic theme tokens, spacing helpers, radii,
  and icon family. Use compact hierarchy and preserve working space.
- Group related controls with spacing and alignment; add borders or elevation
  when they explain a boundary. Avoid decorative badges, gradients, and panels.
- Use monospaced or tabular numerals when alignment aids comparison. Truncate
  only when the full value remains available through a clear interaction.
- Keep selection, focus, danger, and drop targets distinct across themes.
  Pair color with text, icons, or shape when it conveys state.
- Use imagery only when it helps explain content or a workflow. Avoid substitute
  emoji icons and ornamental visuals in everyday working views.

## Make interaction understandable

- Cover applicable hover, pressed, focus, selected, disabled, loading, empty,
  error, and save states. Essential commands must be discoverable without hover.
- Keep errors contextual and actionable. Show what can happen next in empty
  states; preserve useful content while refreshing.
- Give dragged items visible lift and valid destinations clear emphasis.
  Keep neighboring items and the selected item visually anchored during reordering.
- Return focus to the trigger when a transient surface closes. Make Tab order
  follow task order and provide visible focus and usable pointer targets.
- Apply reversible actions immediately with undo or another clear recovery path.
  Keep dirty, saving, saved, and failed feedback stable in width and position.

## Use motion purposefully

Animate to explain feedback, continuity, hierarchy, or a state change. Keep
transitions brief and interruptible; use springs when they clarify direct
manipulation. Reveal panels from a meaningful location and keep drop-target
emphasis from moving unrelated content.

Use the current GPUI Kit APIs and follow the
[performance guidance](../gpui/references/performance.md). Avoid expensive
relayout on each frame, perpetual decoration, and delays before commands take
effect. Progress indicators should run only while work is pending. Preserve a
usable static state and respect reduced-motion support where available.

## Verify the experience

Check affected mouse and keyboard paths, overlay dismissal and focus return,
loading and failure states, and relevant narrow/wide windows, font sizes,
display scales, and light/dark themes. Report automated checks separately from
manual visual acceptance; do not claim visual acceptance from compilation alone.
