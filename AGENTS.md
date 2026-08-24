# Castle

Castle is a Rust note-taking and Kanban app built with GPUI and GPUI Component.

## Working Agreement

- Before editing, read the closest implementation, focused tests, public re-export seam, and Cargo-locked API source. Current source and compiler output outrank examples or documentation from another revision.

## Code Style

- Don't use unwrap.
- Don't comment obvious logic.
- Use precise domain names and established GPUI terminology. Name render helpers after meaningful regions and split a module when unrelated state ownership or lifecycle obscures it.

## Code Quality

- Avoid workarounds or hacks. Instead, find a better solution or implement the feature properly.

## Verification

- Add a deterministic regression test before fixing a reproducible bug. Report automated checks and manual visual acceptance separately.
- Run focused formatting, tests, and `cargo clippy --fix --allow-dirty` for related files when applicable; avoid overlapping Cargo jobs.
