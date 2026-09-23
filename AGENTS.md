# Castle

Castle is a Rust note-taking and Kanban app built with GPUI Kit (`gpui-kit`).

## Working Agreement

Before editing, read the nearest implementation, its tests, the re-export
seam, and the component docs. Search current source for signatures; never
translate a React, CSS, or old GPUI example by analogy. For GPUI work, load
the `gpui` skill plus the references it routes to for the task.

## Common failure modes

Avoid these patterns:

- One entity holding unrelated state. Split by behavior ownership and lifecycle, not visual fragments.
- Logic, persistence, or network requests in `render`. Render presents already-owned state.
- Duplicated state drifting from a controlled value. One source of truth unless the copy has a sync and lifecycle contract.
- `cx.notify()` loops from mutating during render, prepaint, observer callbacks, or observer cycles.
- A one-off component variant. Prefer composition or a local exception; add a variant only as a reusable semantic contract.
- Confirming reversible, low-risk actions. Apply with undo or another recovery path instead.

## Code Style

- Precise domain names and GPUI terminology. Name render helpers after regions; split a module when unrelated ownership or lifecycle obscures it.
- Don't use unwrap.
- Don't comment obvious logic.

## Code Quality

- No workarounds or hacks; implement the feature properly.

## Verification

- Add a deterministic regression test before fixing a reproducible bug. Report automated checks and visual acceptance separately.

## Agent bootstrap

Once per checkout or worktree, not before every task:

```sh
make bootstrap
```

After toolchain or dependency changes run `make bootstrap-full`. Both build the MCP server and prepare ignored data under `target\agent-data`.

## Canonical commands

Iterate with the package lane, then run the Fast lane before handoff:

```sh
make check-package PACKAGE=board
make check
make test
make test-mcp
make test-full
```

Use `make check-package PACKAGE=<name>` while iterating and `make check` before handoff. `make test` excludes `shell`; `make test-full` covers all packages. Run `make test-mcp` for MCP changes or `make test-mcp-launcher` for launcher and config changes.

## Safe MCP

The trusted project config uses a local stdio server, an isolated database under `target\agent-data`, and approval for writes. Keep JSON-RPC on stdout, diagnostics on stderr, and never use personal or production data by default.

When MCP behavior is relevant, run:

```sh
make test-mcp
```
