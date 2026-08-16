# Project Overview

This project is a note taking and kanban board app that uses GPUI and GPUI Components writen in rust.

## Tools

Use 'cargo clippy --fix --allow-dirty' for related files when it is applicable to ensure code quality

## Code Style

- Don't use unwrap.
- Don't comment obvious logic.

## Code Quality

- Avoid workarounds or hacks. Instead, find a better solution or implement the feature properly.

## Async and Entity Safety

- Never do any blocking work on the foreground executor. Instead, offload it to the background executor or use a separate Tokio runtime.
- Never await SeaORM or SQLx work directly inside `cx.spawn` or `cx.spawn_in`; those tasks run on GPUI's foreground executor. Spawn database work through the current Tokio runtime, then apply the completed result to GPUI entities on the foreground executor.
- Don't move SQLx futures to GPUI's background executor. SQLx requires a Tokio runtime context.
