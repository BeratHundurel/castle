# GPUI Performance and Failure Modes

Load this reference when changing rendering, entity ownership, retained state, collections, closures, notifications, or animation.

## Performance rules

- Apply a coherent state change before notifying, and notify the narrowest owning entity whose presentation changed. Do not broadcast or notify intermediate states unnecessarily, especially during high-frequency pointer or drag updates.
- Virtualize long collections and render only the visible range. Preserve stable domain identity independently of the current visible index.
- Avoid cloning large strings or collections solely to satisfy a closure. Capture stable handles, weak entities, reference-counted immutable data, or a smaller owned value when their lifecycle matches the callback.
- Measure before adding a cache. Every cache must justify its cost and have a clear invalidation owner tied to the inputs or revision that make the value valid.
- Keep animation work bounded, avoid expensive relayout on every frame, and honor reduced motion. Animation must not delay input or completion.

## Review contract

Before finishing, identify:

1. The behavior owner, presentation owner, and entity that owns each state change and notification.
2. The retained identity and lifecycle of entities, subscriptions, focus handles, and callbacks.
3. The expected collection size and whether rendering is bounded to the visible range.
4. Any large value captured by a closure and why that ownership is necessary.
5. Every cache, its measured motivation, inputs, invalidation owner, and stale-data behavior.
6. The pointer, keyboard, focus, disabled, and accessibility contract.
7. The lowest public interaction test that would fail on regression.
