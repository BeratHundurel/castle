# Project Overview

This project is a note taking and kanban board app that uses GPUI and GPUI Components writen in rust.

## Documentation

You can find the source code of the GPUI components library in /docs.
Don't make edits, it's for reference only.

### Tools

Use 'cargo check' and 'cargo clippy --fix --allow-dirty' for related files when it is applicable to ensure code quality

### Skills

You can find the skills for the gpui in .agents\skills

### Code Style

Don't use unwrap
Don't comment obvious logic

### Async and Entity Safety

- Never await SeaORM or SQLx work directly inside `cx.spawn` or `cx.spawn_in`; those tasks run on GPUI's foreground executor. Spawn database work through the current Tokio runtime, then apply the completed result to GPUI entities on the foreground executor.
- Don't move SQLx futures to GPUI's background executor. SQLx requires a Tokio runtime context.
- Coalesce repeated workspace, sidebar, Home, and Trash refreshes instead of starting overlapping snapshot queries. Use a generation guard when an older result could overwrite newer state.
- Reuse cached note and board view entities across tab closure. Rapidly destroying timer-heavy editor entities can starve GPUI's foreground scheduling.
- For async lifecycle changes, test repeated churn and verify useful work afterward: database writes, note loading, board data, and sidebar refreshes must still complete.
