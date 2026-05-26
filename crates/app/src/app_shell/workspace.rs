use super::*;

impl AppShell {
    pub(super) fn refresh_workspace(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let projects = Project::find().all(&*db).await?;
            let boards = Board::find().all(&*db).await?;

            let project_choices: Vec<ProjectChoice> = projects
                .iter()
                .map(|project| ProjectChoice {
                    id: project.id as u32,
                    name: SharedString::from(project.name.clone()),
                })
                .collect();

            let board_choices: Vec<BoardChoice> = boards
                .into_iter()
                .map(|board| {
                    let project_name = board.project_id.and_then(|project_id| {
                        projects
                            .iter()
                            .find(|project| project.id == project_id)
                            .map(|project| SharedString::from(project.name.clone()))
                    });

                    BoardChoice {
                        id: board.id as u32,
                        title: SharedString::from(board.title),
                        project_name,
                    }
                })
                .collect();

            this.update(cx, |this, cx| {
                this.projects = project_choices;
                this.boards = board_choices;
                cx.notify();
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    pub(super) fn create_note(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let title = SharedString::from("Untitled note");
        let now = now_ts();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let inserted = note::ActiveModel {
                title: Set(title.to_string()),
                project_id: Set(project_id.map(|id| id as i64)),
                file_path: Set(None),
                cached_content: Set(DEFAULT_NOTE.to_string()),
                file_missing_since: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&*db)
            .await
            .ok()?;

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            inserted.id as u32,
                            project_id,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |_, cx| SidebarView::list_projects(cx));
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(super) fn open_note_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open note file".into()),
        });
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let paths = paths.await.ok()?.ok()??;
            let path = paths.first()?.clone();
            let content = read_to_string(&path).ok()?;
            let path_string = path.display().to_string();
            let title = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled note")
                .to_string();

            let existing = Note::find()
                .filter(note::Column::FilePath.eq(path_string.clone()))
                .one(&*db)
                .await
                .ok()
                .flatten();

            let note = if let Some(existing) = existing {
                note::ActiveModel {
                    id: Set(existing.id),
                    title: Set(existing.title),
                    file_path: Set(Some(path_string)),
                    cached_content: Set(content),
                    file_missing_since: Set(None),
                    updated_at: Set(now_ts()),
                    ..Default::default()
                }
                .update(&*db)
                .await
                .ok()?
            } else {
                let now = now_ts();
                note::ActiveModel {
                    title: Set(title),
                    project_id: Set(None),
                    file_path: Set(Some(path_string)),
                    cached_content: Set(content),
                    file_missing_since: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&*db)
                .await
                .ok()?
            };

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.open_note_tab(
                            note.id as u32,
                            None,
                            SharedString::from(note.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |_, cx| SidebarView::list_projects(cx));
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(super) fn create_board(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let inserted = board::ActiveModel {
                title: Set("Board".to_string()),
                project_id: Set(project_id.map(|id| id as i64)),
                ..Default::default()
            }
            .insert(&*db)
            .await
            .ok()?;

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.open_board_tab(
                            inserted.id as u32,
                            project_id,
                            SharedString::from(inserted.title),
                            window,
                            cx,
                        );
                        this.sidebar
                            .update(cx, |_, cx| SidebarView::list_projects(cx));
                        this.refresh_workspace(cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }
}
