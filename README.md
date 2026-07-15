# Castle

Castle is a native note-taking and kanban board app built with Rust, [GPUI](https://www.gpui.rs/), and [GPUI Components](https://github.com/longbridge/gpui-component).

## Download

Download the latest Windows release from the repository's [Releases page](https://github.com/BeratHundurel/castle/releases/latest).

- Intel or AMD PC: choose the `windows-x86_64` file.
- Windows on ARM PC: choose the `windows-arm64` file.
- Use the `.msi` for a normal installation or the `.exe` as a standalone app.

Windows may show a SmartScreen warning because the current release artifacts are not code-signed.

## Notes

Write focused notes with Markdown and code blocks.

![A note in Castle](images/note.png)

![A note with a code block in Castle](images/note-with-code.png)

## Boards

Organize work visually with kanban boards, cards, labels, checklists, and due dates.

![A kanban board in Castle](images/board.png)

## Run locally

Castle is currently developed for Windows. Install the Rust toolchain, then run:

```sh
cargo run
```

Maintainers can publish a new version by following [RELEASING.md](RELEASING.md).
