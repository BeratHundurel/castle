# Layout, Measurement, and Scrolling

Load this reference when changing geometry-dependent behavior, overlays, virtualization, editors, resize handles, charts, alignment, overflow, or scrolling.

## Prefer layout over measurement

Most UI should use GPUI layout rather than measuring itself. Measurement is a deep behavior tool for popups, virtualization, editors, resize handles, charts, and similar components whose correctness depends on resolved geometry.

- Put measurement and geometry in the layer that owns the behavior.
- Observe bounds in prepaint only when ordinary layout cannot express the relationship.
- Never mutate unrelated application state every prepaint.
- Treat measured data as frame- or revision-scoped. It can become stale after typography, rem size, width, theme, display scale, or content changes.
- Centralize shared geometry such as popup flipping and viewport clamping so every overlay follows the same edge policy.

When measurement must persist beyond the current phase, define the revision or complete set of inputs that validates it. Notify only the behavior owner when a meaningful geometry result changes; do not create a prepaint-notify loop.

## Construct alignment invariants

Prefer construction over correction. Sibling regions should consume the same spacing token or shared inset instead of repeating equivalent literals.

Add geometry assertions or visual regression coverage for critical repeated edges, columns, and gaps. Exercise more than the default window configuration: rem zoom and display scaling can turn fractional coordinates into a one-physical-pixel drift even when the default screenshot looks aligned.

Measure the resolved result when reviewing precision, but do not encode a measured correction as a raw `px(...)` nudge. Trace the mismatch to duplicated padding, nested insets, border ownership, font metrics, or rounding, then fix the structural owner.

## Assign one scroll owner

Every scrollable region must have one owner.

- In flex layouts, apply `min_w_0()` or `min_h_0()` to the flexible child that is allowed to shrink.
- Avoid accidental nested scrolling. If nesting is intentional, define the viewport and axis owned by each region and route wheel input to the intended axis.
- Preserve platform and wasm differences when an input or scrolling API is not portable.
- Keep content inset inside the scroll owner.

Attach `Scrollable` to the element that owns the full panel, editor, or window viewport so its scrollbar resolves against the region edge. Do not wrap the scroll owner in a padded container merely to inset its content. A scrollbar floating between content and the panel boundary usually reveals the wrong scroll owner or padding on the wrong layer.

## Review contract

Before finishing, identify:

1. The layout and overflow owner for each affected region.
2. Why ordinary GPUI layout is insufficient for every measurement.
3. The phase or revision that bounds each measured value's validity.
4. The shared owner for reusable overlay and viewport geometry.
5. The single scroll owner, axis, wheel-routing behavior, shrinkable flex child, and content-inset location.
6. Geometry or visual coverage at relevant window sizes, rem zoom, and display scales.
