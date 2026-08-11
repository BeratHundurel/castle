# Castle architecture

Castle is a native GPUI application backed by SQLite. The workspace keeps persistence,
protocol, and UI concerns in separate crates so that dependency direction can be checked at
compile time.

## Dependency direction

```text
app ────────────┐
                ├──> storage ──> entity
castle-mcp ─────┘       │
                        └──────> migration
```

- `app` owns GPUI entities, interaction state, rendering, shortcuts, and conversion from
  storage records into GPUI values such as `SharedString`.
- `castle-mcp` is a protocol adapter. It translates MCP requests to storage commands and
  storage records to the existing serialized response types.
- `storage` owns database bootstrap, migrations, queries, mutations, validation, ordering,
  timestamps, soft deletion, transactions, and link indexing.
- `entity` and `migration` are persistence implementation details. Production code in `app`
  and `castle-mcp` must not import them or SeaORM directly. The architecture regression test
  in `crates/storage/tests/architecture.rs` enforces that boundary.

The storage boundary returns ordinary Rust records and identifiers. Neither GPUI-specific
types nor MCP transport types belong below that boundary. A separate domain crate should only
be introduced if a substantial GPUI-independent model emerges naturally.

## Store lifecycle

`Store::connect(StoreOptions)` is the single production bootstrap path. It configures the
SeaORM connection pool and runs pending migrations before returning a cloneable `Store`.
Castle and Castle MCP use the same options and bootstrap behavior.

Storage feature modules own their transaction boundaries. A command that changes several
tables performs all related writes in one transaction; callers do not reproduce validation or
attempt to repair partial writes.

## Mutation origins and external revisions

Every protocol-facing mutation is selected through
`Store::mutations(MutationOrigin::{LocalApp, ExternalAgent})`.

- `LocalApp` preserves Castle's existing behavior and does not advance external-change
  revisions.
- `ExternalAgent` advances the command's built-in change domain in the same transaction as
  the data mutation.

Change domains are private storage knowledge. Callers cannot supply revision columns. A
command affecting links can advance workspace, board, note, and link revisions as required by
the compatibility mapping. If either the data write or revision update fails, the entire
transaction rolls back.

## GPUI and Tokio handoff

`AppServices` is Castle's immutable GPUI global. It privately owns the cloneable `Store`, the
Tokio runtime handle, application paths, and board-layout persistence service. Views access
those capabilities through narrow accessors rather than storing database globals.

The execution rules are:

1. Construct and poll database futures on the captured Tokio runtime. `spawn_store` is the
   standard bridge for this work.
2. Await the Tokio join handle from a GPUI foreground task, then update GPUI entities on the
   foreground executor.
3. Run CPU-heavy analysis and ordinary file processing on GPUI's background executor.
4. Never await SeaORM or SQLx work directly inside `cx.spawn` or `cx.spawn_in`, and never move
   SQLx futures to GPUI's background executor.

Long-lived views keep related state in plain structs inside their existing GPUI entity.
Request trackers own cancellation tasks and generation tokens so stale work cannot overwrite
newer state. A new GPUI entity is warranted only when a component needs independent rendering,
focus, or lifecycle.

## Compatibility contract

Architecture changes must preserve user-visible behavior, SQLite tables and migrations, saved
data, note paths, settings, shortcuts, and MCP request and response JSON. Legacy database table
names remain internal even when source-domain records use `BoardList` and `BoardCard` names.
