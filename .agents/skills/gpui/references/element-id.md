# ElementId

`ElementId` is a unique identifier for a GPUI element. It is required for elements that need:
- Mouse event handling (`on_click`, `on_hover`, etc.)
- State storage via `window.use_keyed_state`
- Interaction tracking

## Making an Element Stateful

Call `.id()` on a `div()` to create a `Stateful<Div>`:

```rust
div().id("my-element")          // ElementId from &str
div().id(42usize)               // ElementId from usize
div().id(ElementId::from(idx))  // Explicit
```

Without `.id()`, a div cannot receive mouse events or store state.

## Accepted Types

```rust
impl Into<ElementId> for &str      // "my-id"
impl Into<ElementId> for String    // String::from("my-id")
impl Into<ElementId> for usize     // 0, 1, 2, ...
impl Into<ElementId> for u64
impl Into<ElementId> for SharedString
```

## Uniqueness Rules

IDs must be unique within the same **stateful parent's scope** — not globally. GPUI builds a `GlobalElementId` by chaining parent IDs:

```rust
div().id("app").child(
    div().id("list1").children(vec![
        div().id(1usize).child("Item 1"),  // GlobalId: ["app", "list1", 1]
        div().id(2usize).child("Item 2"),  // GlobalId: ["app", "list1", 2]
    ])
).child(
    div().id("list2").children(vec![
        div().id(1usize).child("Item 1"),  // GlobalId: ["app", "list2", 1] — no conflict
    ])
)
```

Items in different parent scopes can reuse simple IDs (integers, short strings).

## In Component Structs

Components always store `id: ElementId` and pass it in `new()`:

```rust
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    base: Stateful<Div>,
    // ...
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            base: div().id(id),  // id applied to base
            // ...
        }
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.base  // already has .id() applied
            .on_click(/* ... */)
    }
}
```

## Usage at Call Sites

```rust
// Use unique string IDs for named components
Button::new("save-btn").label("Save")
Button::new("cancel-btn").label("Cancel")

// Use stable domain IDs in retained or reorderable lists
for item in items {
    div().id(item.id)
}

// Stateful controls receive retained entities, not string IDs
Input::new(&search_input)
Select::new(&country_select)
```

Numeric and index-based IDs remain valid API inputs, but use a list index only
when identity is intentionally positional, the collection cannot reorder, and
no retained interaction state must follow an item across renders.
