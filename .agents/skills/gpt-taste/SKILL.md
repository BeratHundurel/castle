---
name: gpt-taste
description: Create expressive, premium native GPUI interfaces and purposeful motion for Castle. Use when a desktop surface needs stronger spatial composition, direct-manipulation feedback, distinctive interaction patterns, or higher visual ambition without web technologies.
---

# Expressive Native UX and Motion

Create memorable native application interfaces without sacrificing clarity, keyboard access, or responsiveness. This skill applies to Rust, GPUI, and GPUI Components. Do not prescribe React, HTML, CSS, Tailwind, GSAP, browser scrolling, marketing-page frameworks, or web assets.

Read the `gpui` skill and relevant references before implementing framework behavior. Verify APIs in `/docs` and existing project code.

## 1. Establish the interaction concept

Before code, write a short design plan containing:

1. The primary user action and the state change it causes.
2. One spatial composition selected for the surface.
3. Up to three distinctive interaction patterns.
4. The reason for every animation.
5. The fallback for reduced motion, narrow windows, and keyboard-only use.

Choose based on the task, not randomness. A note editor, kanban board, command palette, and settings page should not share the same composition.

## 2. Spatial composition vocabulary

Use one dominant composition and keep supporting regions quieter:

- **Focused canvas:** content occupies the center; tools appear contextually at edges.
- **Asymmetric workspace:** primary content receives most width; inspector or metadata forms a narrower companion region.
- **Layered command surface:** transient palette or switcher appears above dimmed context while preserving spatial orientation.
- **Horizontal board:** fixed column rhythm with intentional horizontal scrolling and strong drag targets.
- **Master-detail:** stable collection and focused detail with resizable or collapsible separation.
- **Progressive inspector:** advanced properties appear only for the selected item.
- **Stacked history:** recent or recoverable items overlap or group to communicate time and reversibility.

Avoid defaulting to three equal cards, a permanent sidebar for every view, centered empty space, or symmetrical panels that imply equal importance when tasks are not equal.

## 3. Typography and hierarchy

- Keep primary workspace titles concise and normally within one or two lines.
- Use width, weight, tone, and spacing before extreme font size.
- Keep supporting copy subordinate to the task and remove decorative meta-labels.
- Align counts and timestamps for scanning.
- Make selected, dirty, overdue, failed, and disabled states distinct without shouting.
- Preserve content width and editor readability at wide window sizes.

## 4. Direct-manipulation motion

Motion is valid only for feedback, spatial continuity, hierarchy, or state transition.

- **Drag lift:** distinguish the picked-up card from its origin and show the valid destination before commit.
- **Reorder continuity:** move neighboring items predictably so the destination remains understandable.
- **Panel transition:** reveal contextual panels from their spatial source; return focus when they close.
- **Selection continuity:** keep selection visually anchored while results or columns update.
- **Save feedback:** transition between dirty, saving, saved, and failed without changing layout width.
- **Destructive action:** favor reversible removal with a clear undo period over theatrical confirmation motion.

Use short, interruptible transitions. Do not make users wait for animation. Avoid perpetual motion, ornamental parallax, cursor trails, scroll hijacking, and animation that changes layout on every frame.

Use only GPUI animation capabilities already present in the dependency graph. Verify their real signatures before use. Never add a browser animation library.

## 5. Native interaction patterns

- **Command palette:** keyboard-first search, stable result selection, action shortcuts, and clear unavailable states.
- **Contextual toolbar:** reveal actions near selection while keeping a keyboard-accessible command path.
- **Drag target expansion:** enlarge or emphasize a valid drop zone while dragging without moving unrelated content unexpectedly.
- **Inline disclosure:** expand metadata or editing controls in place when the task is brief and reversible.
- **Inspector panel:** use for persistent properties that users compare while navigating content.
- **Undo toast:** show transient confirmation for reversible actions; keep errors until resolved or dismissed.
- **Skeleton geometry:** mirror the final shape for perceptible loading; avoid generic spinners for large content regions.
- **Empty composition:** provide one clear next action and a restrained visual cue tied to the real workflow.

## 6. Visual material

- Use semantic theme tokens throughout.
- Choose one accent and one radius logic for the surface.
- Use cards only where containment, selection, or drag behavior needs a boundary.
- Prefer border, tone, spacing, and overlap over heavy shadows.
- Keep decorative textures and imagery out of high-frequency work areas.
- Use the existing icon family with consistent sizing and optical alignment.
- Never use emojis as interface icons.

## 7. Focus and input integrity

- Track focus on every keyboard-interactive region.
- Use actions, key bindings, and key contexts for commands.
- Make hover, pressed, focused, selected, and disabled states independently legible.
- Do not hide essential actions until hover without another discoverable route.
- Stop pointer-event propagation only where nested behaviors genuinely conflict.
- Preserve focus and selection through async updates whenever the underlying item remains valid.

## 8. Performance rules

- Keep high-frequency drag and pointer updates local and cheap.
- Avoid rebuilding full board or search result trees for a single hover change.
- Virtualize large collections where an existing component supports it.
- Keep stable `ElementId` values for stateful elements.
- Run SeaORM and SQLx work on Tokio, never directly inside GPUI foreground tasks or on GPUI's background executor.

## 9. Pre-flight

- [ ] The composition reflects the primary user task.
- [ ] Each animation has a stated functional reason.
- [ ] Interactions remain usable with reduced motion.
- [ ] Mouse, keyboard, focus, and drag states are complete.
- [ ] Narrow and wide window behavior is intentional.
- [ ] Theme tokens, spacing, iconography, and radii are coherent.
- [ ] Transitions are interruptible and never delay the user.
- [ ] No web-stack concepts or APIs were introduced.
- [ ] Relevant Rust checks pass.
