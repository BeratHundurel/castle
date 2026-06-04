# Missing Baseline Features

This is a working backlog of features that are expected in a private note-taking and kanban board app like Castle. It is based on the current code surface in `crates/app`: projects, standalone notes, boards, tabs, Markdown editing, sidebar filtering, themes, SQLite persistence, and file-backed Markdown notes are already present.

## P0 - Data Safety And Recovery

- Add trash/archive with restore instead of immediate permanent deletion.
- Add periodic backup/export of the SQLite database and note files.
- Add restore/import from backup.
- Add note version history or snapshots, at least for Markdown notes.
- Persist user preferences such as selected theme, sidebar collapsed state, window size, and last opened tabs.

## P0 - Core Findability

- Add full-text search across note contents, note titles, board titles, list titles, and board entry text.
- Add a quick switcher for opening notes and boards without using the sidebar.
- Add a command palette for common actions: new note, new board, save, search, switch theme, open file, close tabs.
- Add recent items and pinned/favorite notes or boards.
- Add sort controls for sidebar items: manual, title, created date, updated date.
- Add empty states for no projects, no search results, empty boards, and empty notes.

## P0 - Board Essentials

- Allow board entries to be opened, edited, and deleted after creation.
- Display entry descriptions somewhere in the board UI or entry detail view. They are stored but currently not shown on cards.
- Persist ordering of entries within a list and across lists. Only list/card order is currently persisted.
- Support moving entries within the same list, not only between different lists.
- Add labels or colors for board entries.
- Add due dates, reminders, or lightweight status metadata for entries.
- Add checklists inside entries.
- Add duplicate list and duplicate entry actions.

## P1 - Note-Taking Essentials

- Add note linking with `[[wikilinks]]` or another internal link format.
- Add backlinks and linked references.
- Add tags and tag-based filtering.
- Add templates for common note types.
- Add duplicate note action.
- Add note outline/table-of-contents navigation for Markdown headings.
- Add task checkbox support with a task summary view.
- Add Markdown paste handling for images and file drops.
- Add export options: Markdown folder, HTML, PDF, and maybe DOCX.
- Add print support.

## P1 - App Ergonomics

- Add a settings/preferences screen.
- Add keyboard shortcuts for common app-level actions, with a shortcut reference screen.
- Add drag-and-drop organization in the sidebar for moving notes and boards between projects.
- Add project rename, delete, archive, and reorder.
- Add project-level counts or updated timestamps.
- Add a status/activity area for saves, errors, and background operations.
- Add tab restore on launch.

## P1 - Import, Export, And Interop

- Import a folder of Markdown files as a project.
- Export a project as a folder with Markdown notes and board data.
- Support drag-and-drop opening of Markdown files.
- Support copying board entries as Markdown.
- Support sharing/copying internal links to notes, boards, lists, and entries.
- Add data location management so users can choose where Castle stores its database and note files.

## P2 - Privacy And Sync

- Optional app lock or local encryption for a private notes app.
- Manual sync target support, such as a chosen folder that can be synced by Git, Syncthing, Dropbox, or iCloud.
- Conflict detection for externally edited Markdown files.

## P2 - Quality And Maintenance

- Add migration tests for existing databases.
- Add seeded sample data for manual QA.

