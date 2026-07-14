use gpui::{Context, SharedString};
use gpui_component::{IconName, ThemeRegistry};

use crate::app_shell::AppShell;
use crate::command_palette::{PaletteCommand, PaletteCommandKind, SearchablePaletteCommand};

const COMMAND_PALETTE_RESULT_LIMIT: usize = 18;

impl AppShell {
    pub(crate) fn rebuild_command_palette_workspace_commands(&mut self) {
        let note_commands = self.notes.iter().map(|note| {
            let project_name = note
                .project_name
                .clone()
                .unwrap_or_else(|| "Standalone".into());

            searchable_command(PaletteCommand {
                label: SharedString::from(format!("Go to: {}", note.title)),
                subtitle: SharedString::from(format!("Note - {project_name}")),
                icon: IconName::BookOpen,
                kind: PaletteCommandKind::OpenNote {
                    note_id: note.id,
                    project_id: note.project_id,
                    title: note.title.clone(),
                },
            })
        });

        let board_commands = self.boards.iter().map(|board| {
            let project_name = board
                .project_name
                .clone()
                .unwrap_or_else(|| "Standalone".into());

            searchable_command(PaletteCommand {
                label: SharedString::from(format!("Go to: {}", board.title)),
                subtitle: SharedString::from(format!("Board - {project_name}")),
                icon: IconName::LayoutDashboard,
                kind: PaletteCommandKind::OpenBoard {
                    board_id: board.id,
                    project_id: board.project_id,
                    title: board.title.clone(),
                },
            })
        });

        self.command_palette.workspace_commands = note_commands.chain(board_commands).collect();
    }

    pub(crate) fn command_palette_commands(&self) -> Vec<PaletteCommand> {
        let query = self.command_palette.query.trim().to_lowercase();
        let explicit_new_command = new_command(&self.command_palette.query);
        let project_label = self
            .active_project_id
            .and_then(|id| self.projects.iter().find(|project| project.id == id))
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "Standalone".into());

        let mut commands = Vec::new();

        if let Some(command) = explicit_new_command.clone() {
            match command {
                NewCommand::Any(title) => {
                    commands.push(new_note_command(
                        self.active_project_id,
                        title.clone(),
                        project_label.clone(),
                    ));
                    commands.push(new_board_command(
                        self.active_project_id,
                        title,
                        project_label.clone(),
                    ));
                }
                NewCommand::Note(title) => {
                    commands.push(new_note_command(
                        self.active_project_id,
                        title,
                        project_label.clone(),
                    ));
                }
                NewCommand::Board(title) => {
                    commands.push(new_board_command(
                        self.active_project_id,
                        title,
                        project_label.clone(),
                    ));
                }
            }
        }

        commands.extend([
            PaletteCommand {
                label: "New tab".into(),
                subtitle: "Open an empty chooser tab".into(),
                icon: IconName::Plus,
                kind: PaletteCommandKind::NewTab,
            },
            PaletteCommand {
                label: "New note".into(),
                subtitle: SharedString::from(format!("Create in {project_label}")),
                icon: IconName::BookOpen,
                kind: PaletteCommandKind::NewNote {
                    project_id: self.active_project_id,
                    title: "Untitled note".to_string(),
                },
            },
            PaletteCommand {
                label: "New board".into(),
                subtitle: SharedString::from(format!("Create in {project_label}")),
                icon: IconName::LayoutDashboard,
                kind: PaletteCommandKind::NewBoard {
                    project_id: self.active_project_id,
                    title: "Board".to_string(),
                },
            },
            PaletteCommand {
                label: "Open note file".into(),
                subtitle: "Choose a markdown or text file".into(),
                icon: IconName::FolderOpen,
                kind: PaletteCommandKind::OpenFile,
            },
            PaletteCommand {
                label: "Open settings".into(),
                subtitle: SharedString::from(format!(
                    "Change app preferences ({})",
                    settings_shortcut()
                )),
                icon: IconName::Settings2,
                kind: PaletteCommandKind::OpenSettings,
            },
            PaletteCommand {
                label: "Switch theme".into(),
                subtitle: SharedString::from(format!(
                    "Preview available themes ({})",
                    theme_switcher_shortcut()
                )),
                icon: IconName::Palette,
                kind: PaletteCommandKind::SwitchTheme,
            },
            PaletteCommand {
                label: "Close all tabs".into(),
                subtitle: "Return to a new chooser tab".into(),
                icon: IconName::Close,
                kind: PaletteCommandKind::CloseAllTabs,
            },
            PaletteCommand {
                label: "Search workspace".into(),
                subtitle: SharedString::from(format!(
                    "Full-text search ({})",
                    workspace_search_shortcut()
                )),
                icon: IconName::Search,
                kind: PaletteCommandKind::SearchWorkspace,
            },
        ]);

        if query.is_empty() || explicit_new_command.is_some() {
            let remaining = COMMAND_PALETTE_RESULT_LIMIT.saturating_sub(commands.len());
            commands.extend(
                self.command_palette
                    .workspace_commands
                    .iter()
                    .take(remaining)
                    .map(|entry| entry.command.clone()),
            );
            return commands
                .into_iter()
                .take(COMMAND_PALETTE_RESULT_LIMIT)
                .collect();
        }

        commands.retain(|command| command_matches(command, &query));
        let remaining = COMMAND_PALETTE_RESULT_LIMIT - commands.len();
        commands.extend(
            self.command_palette
                .workspace_commands
                .iter()
                .filter(|entry| entry.search_text.contains(&query))
                .take(remaining)
                .map(|entry| entry.command.clone()),
        );

        commands.truncate(COMMAND_PALETTE_RESULT_LIMIT);
        commands
    }

    pub(crate) fn filtered_theme_names(&self, cx: &mut Context<Self>) -> Vec<SharedString> {
        let query = self.command_palette.query.trim().to_lowercase();

        ThemeRegistry::global(cx)
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.clone())
            .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
            .collect()
    }
}

fn command_matches(command: &PaletteCommand, query: &str) -> bool {
    command.label.to_lowercase().contains(query) || command.subtitle.to_lowercase().contains(query)
}

fn workspace_search_shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+F"
    } else {
        "Ctrl+Shift+F"
    }
}

fn settings_shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+,"
    } else {
        "Ctrl+,"
    }
}

fn theme_switcher_shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+Alt+T"
    } else {
        "Ctrl+Alt+T"
    }
}

fn searchable_command(command: PaletteCommand) -> SearchablePaletteCommand {
    let search_text = format!(
        "{} {}",
        command.label.to_lowercase(),
        command.subtitle.to_lowercase()
    );

    SearchablePaletteCommand {
        command,
        search_text,
    }
}

#[derive(Clone)]
enum NewCommand {
    Any(String),
    Note(String),
    Board(String),
}

fn new_command(query: &str) -> Option<NewCommand> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_lowercase();
    let (title, kind): (&str, fn(String) -> NewCommand) = if lower.starts_with("new:") {
        (trimmed.get(4..)?, NewCommand::Any)
    } else if lower.starts_with("new note:") {
        (trimmed.get(9..)?, NewCommand::Note)
    } else if lower.starts_with("new board:") {
        (trimmed.get(10..)?, NewCommand::Board)
    } else {
        return None;
    };

    let title = title.trim();
    if title.is_empty() {
        None
    } else {
        Some(kind(title.to_string()))
    }
}

fn new_note_command(
    project_id: Option<u32>,
    title: String,
    project_label: SharedString,
) -> PaletteCommand {
    PaletteCommand {
        label: SharedString::from(format!("New note: {title}")),
        subtitle: SharedString::from(format!("Create in {project_label}")),
        icon: IconName::BookOpen,
        kind: PaletteCommandKind::NewNote { project_id, title },
    }
}

fn new_board_command(
    project_id: Option<u32>,
    title: String,
    project_label: SharedString,
) -> PaletteCommand {
    PaletteCommand {
        label: SharedString::from(format!("New board: {title}")),
        subtitle: SharedString::from(format!("Create in {project_label}")),
        icon: IconName::LayoutDashboard,
        kind: PaletteCommandKind::NewBoard { project_id, title },
    }
}
