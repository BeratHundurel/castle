use super::*;

impl BoardView {
    pub(crate) fn toggle_visible_property(
        &mut self,
        property: PropertyKey,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .properties
            .active_view_config
            .visible_properties
            .iter()
            .position(|candidate| candidate == &property)
        {
            self.properties
                .active_view_config
                .visible_properties
                .remove(index);
        } else if self.properties.active_view_config.visible_properties.len() < 3 {
            self.properties
                .active_view_config
                .visible_properties
                .push(property);
        } else {
            self.properties.update_error = Some("A view can show up to three fields".into());
            cx.notify();
            return;
        }
        self.properties.view_config_dirty = true;
        self.properties.update_error = None;
        cx.notify();
    }

    pub(crate) fn move_visible_property(
        &mut self,
        property: &PropertyKey,
        offset: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .properties
            .active_view_config
            .visible_properties
            .iter()
            .position(|candidate| candidate == property)
        else {
            return;
        };
        let target = index.saturating_add_signed(offset);
        if target < self.properties.active_view_config.visible_properties.len() && target != index {
            self.properties
                .active_view_config
                .visible_properties
                .swap(index, target);
            self.properties.view_config_dirty = true;
            cx.notify();
        }
    }

    pub(crate) fn toggle_compact_cards(&mut self, cx: &mut Context<Self>) {
        self.properties.active_view_config.compact_cards =
            !self.properties.active_view_config.compact_cards;
        self.properties.view_config_dirty = true;
        cx.notify();
    }

    pub(crate) fn set_sort(&mut self, property: PropertyKey, cx: &mut Context<Self>) {
        self.properties.active_view_config.sort =
            match self.properties.active_view_config.sort.as_ref() {
                Some(sort) if sort.property == property => Some(ViewSort {
                    property,
                    direction: match sort.direction {
                        SortDirection::Ascending => SortDirection::Descending,
                        SortDirection::Descending => SortDirection::Ascending,
                    },
                }),
                _ => Some(ViewSort {
                    property,
                    direction: SortDirection::Ascending,
                }),
            };
        self.properties.view_config_dirty = true;
        cx.notify();
    }

    pub(crate) fn clear_sort(&mut self, cx: &mut Context<Self>) {
        self.properties.active_view_config.sort = None;
        self.properties.view_config_dirty = true;
        cx.notify();
    }

    pub(crate) fn select_saved_view(&mut self, view_id: Option<i64>, cx: &mut Context<Self>) {
        let config = view_id
            .and_then(|view_id| {
                self.properties
                    .saved_views
                    .iter()
                    .find(|view| view.id == view_id)
            })
            .map(|view| view.config.clone())
            .unwrap_or_else(crate::filters::default_view_config);
        self.properties.active_view_id = view_id;
        self.properties.active_view_config = config.clone();
        self.filters = crate::filters::BoardFilters::from_config(&config);
        self.properties.view_config_dirty = false;
        self.properties.view_panel_open = false;
        let Some(board_id) = self.data.board_id else {
            cx.notify();
            return;
        };
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::set_selected_board_view(
                    &store,
                    i64::from(board_id),
                    view_id,
                )
                .await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => this.properties.update_error = None,
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not remember selected view: {error}").into());
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn start_view_rename(
        &mut self,
        view_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self
            .properties
            .saved_views
            .iter()
            .find(|view| view.id == view_id)
        else {
            return;
        };
        self.properties.renaming_view_id = Some(view_id);
        self.properties.rename_view_input.update(cx, |input, cx| {
            input.set_value(view.name.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn commit_view_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(view_id) = self.properties.renaming_view_id else {
            return;
        };
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::rename_board_view(&store, view_id, name).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(view)) => {
                        if let Some(current) = this
                            .properties
                            .saved_views
                            .iter_mut()
                            .find(|current| current.id == view_id)
                        {
                            *current = view;
                        }
                        this.properties.renaming_view_id = None;
                        this.properties.update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not rename view: {error}").into());
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn create_saved_view(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            self.properties.update_error = Some("Enter a view name".into());
            cx.notify();
            return;
        }
        self.filters
            .sync_config(&mut self.properties.active_view_config);
        let config = self.properties.active_view_config.clone();
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                let view = storage::board::properties::create_board_view(
                    &store,
                    i64::from(board_id),
                    name,
                    config,
                )
                .await?;
                storage::board::properties::set_selected_board_view(
                    &store,
                    i64::from(board_id),
                    Some(view.id),
                )
                .await?;
                Ok::<_, anyhow::Error>(view)
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(view)) => {
                        this.properties.active_view_id = Some(view.id);
                        this.properties.active_view_config = view.config.clone();
                        this.properties.saved_views.push(view);
                        this.properties.view_config_dirty = false;
                        this.properties.new_view_form_open = false;
                        this.properties.update_error = None;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not save view: {error}").into())
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn update_active_view(&mut self, cx: &mut Context<Self>) {
        let Some(view_id) = self.properties.active_view_id else {
            return;
        };
        self.filters
            .sync_config(&mut self.properties.active_view_config);
        let config = self.properties.active_view_config.clone();
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::update_board_view(&store, view_id, config).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(view)) => {
                        if let Some(current) = this
                            .properties
                            .saved_views
                            .iter_mut()
                            .find(|current| current.id == view_id)
                        {
                            *current = view.clone();
                        }
                        this.properties.active_view_config = view.config;
                        this.properties.view_config_dirty = false;
                        this.emit_data_committed(cx, false);
                    }
                    Ok(Err(error)) => {
                        this.properties.update_error =
                            Some(format!("Could not update view: {error}").into())
                    }
                    Err(error) => this.set_property_task_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn set_default_view(&mut self, view_id: i64, cx: &mut Context<Self>) {
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::set_default_board_view(&store, view_id).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if let Ok(Ok(default_view)) = result {
                    for view in &mut this.properties.saved_views {
                        view.is_default = view.id == default_view.id;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn delete_saved_view(&mut self, view_id: i64, cx: &mut Context<Self>) {
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                storage::board::properties::delete_board_view(&store, view_id).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(())) => {
                    this.properties
                        .saved_views
                        .retain(|view| view.id != view_id);
                    this.properties.update_error = None;
                    this.emit_data_committed(cx, false);
                    if this.properties.active_view_id == Some(view_id) {
                        this.select_saved_view(None, cx);
                    } else {
                        cx.notify();
                    }
                }
                Ok(Err(error)) => {
                    this.properties.update_error =
                        Some(format!("Could not delete view: {error}").into());
                    cx.notify();
                }
                Err(error) => {
                    this.set_property_task_error(error, cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}
