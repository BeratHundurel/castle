use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::{Render, canvas};

pub struct WorkflowPage {
    workspace: Entity<WorkflowWorkspace>,
    route: WorkflowRoute,
    focus: FocusHandle,
}
impl WorkflowPage {
    pub fn new(
        workspace: Entity<WorkflowWorkspace>,
        route: WorkflowRoute,
        cx: &mut Context<Self>,
    ) -> Self {
        if route == WorkflowRoute::Mermaid {
            workspace.update(cx, |model, cx| model.prepare_mermaid_preview(cx));
        }
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            workspace,
            route,
            focus: cx.focus_handle(),
        }
    }
}

impl Focusable for WorkflowPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkflowPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.route;
        if route == WorkflowRoute::Mermaid {
            self.workspace.update(cx, |model, _| {
                model.release_mermaid_preview_images_after_frame(window)
            });
        }

        let weak = self.workspace.downgrade();
        let content = self
            .workspace
            .update(cx, |model, cx| model.render_page(route, cx));

        div()
            .id("workflow-page")
            .debug_selector(|| "workflow-page".into())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.route == WorkflowRoute::Editor {
                    this.workspace.update(cx, |model, cx| {
                        model.draft.selected_node = None;
                        cx.notify();
                    });
                }
            }))
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(content)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        weak.update(cx, |model, cx| {
                            let narrow = bounds.size.width < px(960.);
                            let was_narrow = model.route_narrow(route);
                            if was_narrow != narrow {
                                if !was_narrow
                                    && narrow
                                    && route == WorkflowRoute::Editor
                                    && model.draft.selected_node.is_some()
                                {
                                    cx.defer_in(window, |_, _, cx| {
                                        cx.emit(WorkflowWorkspaceEvent::Navigate(
                                            WorkflowRoute::Step,
                                        ))
                                    });
                                }
                                model.set_route_narrow(route, narrow);
                                cx.notify();
                            }
                        })
                        .ok();
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

impl WorkflowWorkspace {
    fn render_page(&self, route: WorkflowRoute, cx: &mut Context<Self>) -> AnyElement {
        let body = match route {
            WorkflowRoute::Overview => self.render_overview(cx),
            WorkflowRoute::Editor => self.render_editor(cx),
            WorkflowRoute::Step => self.render_builder_sidebar(true, cx),
            WorkflowRoute::Mermaid => self.render_mermaid_preview(cx),
            WorkflowRoute::Run => self.render_manual_run(cx),
        };
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_header(route, cx))
            .child(body)
            .into_any_element()
    }

    fn render_header(&self, route: WorkflowRoute, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let name = self.draft.name_input.read(cx).value().trim().to_string();
        let name = if name.is_empty() {
            "Untitled workflow".to_string()
        } else {
            name
        };
        let mut breadcrumb = h_flex()
            .debug_selector(|| "workflow-breadcrumb".into())
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .child(
                Button::new("workflow-breadcrumb-board")
                    .debug_selector(|| "workflow-breadcrumb-board".into())
                    .label("Board")
                    .child(IconName::ChevronRight)
                    .ghost()
                    .small()
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(WorkflowWorkspaceEvent::Board))),
            );

        if route == WorkflowRoute::Overview {
            breadcrumb = breadcrumb.child(
                div()
                    .debug_selector(|| "workflow-breadcrumb-current".into())
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Workflows"),
            );
        } else {
            breadcrumb = breadcrumb.child(
                Button::new("workflow-breadcrumb-overview")
                    .debug_selector(|| "workflow-breadcrumb-overview".into())
                    .label("Workflows")
                    .child(IconName::ChevronRight)
                    .ghost()
                    .small()
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Overview))
                    })),
            );
            if route == WorkflowRoute::Editor {
                breadcrumb = breadcrumb.child(
                    div()
                        .debug_selector(|| "workflow-breadcrumb-current".into())
                        .min_w_0()
                        .max_w(px(320.))
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                );
            } else {
                let title = match route {
                    WorkflowRoute::Step if self.draft.selected_node.is_none() => "Workflow details",
                    WorkflowRoute::Step => "Step settings",
                    WorkflowRoute::Mermaid => "Mermaid preview",
                    WorkflowRoute::Run => "Run saved workflows",
                    _ => unreachable!(),
                };
                breadcrumb = breadcrumb
                    .child(
                        Button::new("workflow-breadcrumb-editor")
                            .debug_selector(|| "workflow-breadcrumb-editor".into())
                            .label(name)
                            .child(IconName::ChevronRight)
                            .ghost()
                            .small()
                            .max_w(px(240.))
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Editor))
                            })),
                    )
                    .child(
                        div()
                            .debug_selector(|| "workflow-breadcrumb-current".into())
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    );
            }
        }

        let mut header = h_flex()
            .id("workflow-header")
            .debug_selector(|| "workflow-header".into())
            .h(px(52.))
            .flex_shrink_0()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .child(breadcrumb);

        if route == WorkflowRoute::Editor {
            let (status_icon, status_color) = if self.draft.saving {
                (IconName::Loader, theme.info)
            } else if self.draft.dirty(cx) {
                (IconName::Asterisk, theme.warning)
            } else {
                (IconName::CircleCheck, theme.success)
            };

            if self.route_narrow(WorkflowRoute::Editor) {
                header = header.child(
                    Button::new("workflow-details")
                        .debug_selector(|| "workflow-details".into())
                        .label("Steps")
                        .ghost()
                        .small()
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(WorkflowWorkspaceEvent::Navigate(WorkflowRoute::Step))
                        })),
                );
            }
            header = header.child(
                h_flex()
                    .debug_selector(|| "workflow-header-actions".into())
                    .gap_2()
                    .child(Icon::new(status_icon).xsmall().text_color(status_color))
                    .child(
                        Button::new("workflow-save")
                            .debug_selector(|| "workflow-save".into())
                            .icon(IconName::HardDrive)
                            .text_color(theme.primary)
                            .ghost()
                            .small()
                            .disabled(self.draft.saving)
                            .on_click(cx.listener(|this, _, _, cx| this.save_workflow(cx))),
                    )
                    .child(self.render_workflow_more(cx)),
            );
        } else {
            if route == WorkflowRoute::Overview && !self.state.new_workflow_open {
                header = header.child(
                    Button::new("workflow-new")
                        .debug_selector(|| "workflow-new".into())
                        .label("New workflow")
                        .small()
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_new_workflow(cx);
                        })),
                );
            }
        }
        header.into_any_element()
    }

    fn render_new_workflow_chooser(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .id("workflow-new-section")
            .debug_selector(|| "workflow-new-section".into())
            .max_w(px(640.))
            .w_full()
            .gap_4()
            .p_6()
            .rounded_md()
            .border_1()
            .border_color(theme.border.opacity(0.72))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Build an automation"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Start from a proven rule, or compose a workflow step by step."),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("workflow-blank")
                            .debug_selector(|| "workflow-blank".into())
                            .label("Start from scratch")
                            .outline()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.new_workflow(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("workflow-cancel-new")
                            .label("Cancel")
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_new_workflow(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child("STARTER RULES"),
            )
            .child(self.render_workflow_templates(cx))
            .into_any_element()
    }

    fn render_overview(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let new_workflow_chooser = self
            .state
            .new_workflow_open
            .then(|| self.render_new_workflow_chooser(cx));
        let workflow_count = self.state.workflows.len();
        let enabled_count = self
            .state
            .workflows
            .iter()
            .filter(|workflow| workflow.enabled)
            .count();
        let records = self.state.workflows.iter().map(|record| {
            let id = record.id;
            let draft = if self.draft.active_id == Some(id) {
                Some(&self.draft)
            } else {
                self.drafts
                    .values()
                    .find(|draft| draft.active_id == Some(id))
            };
            let dirty = draft.is_some_and(|draft| draft.dirty(cx));
            v_flex()
                .id(("workflow-row", id as u64))
                .p_3()
                .gap_3()
                .rounded_md()
                .border_1()
                .border_color(theme.border.opacity(0.72))
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new(("workflow-open", id as u64))
                                .label(record.name.clone())
                                .ghost()
                                .small()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_workflow(id, window, cx)
                                })),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(if dirty {
                                    theme.warning
                                } else if record.enabled {
                                    theme.success
                                } else {
                                    theme.muted_foreground
                                })
                                .child(if dirty {
                                    "Unsaved draft"
                                } else if record.enabled {
                                    "Live"
                                } else {
                                    "Off"
                                }),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(self.workflow_summary(&record.definition)),
                )
                .into_any_element()
        });
        let drafts = std::iter::once(&self.draft)
            .chain(self.drafts.values())
            .filter(|draft| draft.active_id.is_none() && draft.key != 0)
            .map(|draft| {
                let key = draft.key;
                Button::new(("workflow-resume", key))
                    .label(format!("{} · Unsaved", draft.name_input.read(cx).value()))
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| this.resume_draft(key, cx)))
                    .into_any_element()
            });
        div().debug_selector(|| "workflow-overview-scroll".into()).flex_1().min_h_0()
            .child(v_flex().id("workflow-overview-scroll").size_full().overflow_y_scrollbar().child(v_flex().w_full().max_w(px(920.)).mx_auto().p_6().gap_6()
                .child(
                    v_flex()
                        .id("workflow-overview-intro")
                        .debug_selector(|| "workflow-overview-intro".into())
                        .gap_2()
                        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Automate the routine work"))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(if workflow_count == 0 {
                                    "Create a rule that reacts when work changes on this board."
                                        .to_string()
                                } else {
                                    format!(
                                        "{enabled_count} live of {workflow_count} workflow{} on this board.",
                                        if workflow_count == 1 { "" } else { "s" }
                                    )
                                }),
                        ),
                )
                .when_some(new_workflow_chooser, |this, chooser| {
                    this.child(chooser)
                })
                .when_some(self.state.overview_notice.clone(), |this, notice| {
                    this.child(div().text_sm().text_color(theme.muted_foreground).child(notice))
                })
                .when(self.state.workflows.is_empty(),|this|this.child(
                    v_flex()
                        .id("workflow-starter-panel")
                        .debug_selector(|| "workflow-starter-panel".into())
                        .gap_3()
                        .p_6()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border.opacity(0.72))
                        .child(div().font_weight(FontWeight::MEDIUM).child("A workflow has three simple parts"))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("When something happens, optionally check a condition, then take an action. Starter rules give you a useful shape to customize."),
                        )
                        .child(
                            Button::new("workflow-starter-new")
                                .debug_selector(|| "workflow-starter-new".into())
                                .label("Choose a starter rule")
                                .primary()
                                .on_click(cx.listener(|this, _, _, cx| this.open_new_workflow(cx))),
                        ),
                ))
                .when(!self.state.workflows.is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.muted_foreground)
                            .child("WORKFLOWS"),
                    )
                })
                .children(records)
                .when(self.drafts.values().any(|draft| draft.active_id.is_none()) || (self.draft.active_id.is_none() && self.draft.key != 0), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.muted_foreground)
                            .child("UNSAVED DRAFTS"),
                    )
                })
                .children(drafts))).into_any_element()
    }

    fn render_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let canvas = self.render_workflow_graph(cx);
        let body = h_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .debug_selector(|| "workflow-canvas-scroll".into())
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        div()
                            .id("workflow-canvas-scroll")
                            .relative()
                            .size_full()
                            .overflow_scroll()
                            .track_scroll(&self.draft.canvas_scroll)
                            .child(canvas)
                            .child({
                                let weak = cx.entity().downgrade();
                                gpui_kit::canvas(
                                    move |bounds, _, cx| {
                                        weak.update(cx, |model, cx| {
                                            let width = f32::from(bounds.size.width);
                                            let height = f32::from(bounds.size.height);
                                            if (model.canvas_width - width).abs() > 0.5
                                                || (model.canvas_height - height).abs() > 0.5
                                            {
                                                model.canvas_width = width;
                                                model.canvas_height = height;
                                                cx.notify();
                                            }
                                        })
                                        .ok();
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full()
                            }),
                    )
                    .scrollbar(
                        &self.draft.canvas_scroll,
                        gpui_kit::component::scroll::ScrollbarAxis::Both,
                    )
                    .when_some(self.draft.error.clone(), |this, error| {
                        this.child(
                            div()
                                .debug_selector(|| "workflow-editor-error".into())
                                .absolute()
                                .top_2()
                                .left_4()
                                .right_4()
                                .p_2()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().danger)
                                .bg(cx.theme().popover)
                                .text_sm()
                                .text_color(cx.theme().danger)
                                .child(error),
                        )
                    }),
            )
            .when(!self.route_narrow(WorkflowRoute::Editor), |this| {
                this.child(self.render_builder_sidebar(false, cx))
            });
        body.into_any_element()
    }

    fn render_add_step(&self, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new("workflow-add-step")
            .debug_selector(|| "workflow-add-step".into())
            .label("Add step")
            .icon(IconName::Plus)
            .primary()
            .small()
            .dropdown_menu(move |menu, _, _| {
                let options = [
                    (
                        "When · Trigger",
                        WorkflowNodeKind::Trigger {
                            trigger: WorkflowTrigger::Manual,
                        },
                    ),
                    (
                        "If · Condition",
                        WorkflowNodeKind::Condition {
                            condition: WorkflowCondition::Always,
                        },
                    ),
                    (
                        "Then · Action",
                        WorkflowNodeKind::Action {
                            action: WorkflowAction::MarkComplete,
                        },
                    ),
                    (
                        "Branch · Multiple outcomes",
                        WorkflowNodeKind::Branch {
                            cases: vec![WorkflowBranchCase {
                                id: "case-1".into(),
                                label: "Done list".into(),
                                condition: WorkflowCondition::ListRoleIs {
                                    role: workflow::ListWorkflowRole::Done,
                                },
                            }],
                        },
                    ),
                    (
                        "End · Finish",
                        WorkflowNodeKind::End {
                            label: Some("Done".into()),
                        },
                    ),
                ];
                options.into_iter().fold(menu, |menu, (label, kind)| {
                    let weak = weak.clone();
                    menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| this.add_workflow_node(kind.clone(), cx))
                            .ok();
                    }))
                })
            })
            .into_any_element()
    }

    fn render_workflow_more(&self, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        Button::new("workflow-more")
            .label("More")
            .ghost()
            .small()
            .dropdown_menu(move |menu, _, _| {
                [
                    ("Mermaid preview", WorkflowRoute::Mermaid),
                    ("Run saved manual workflows", WorkflowRoute::Run),
                ]
                .into_iter()
                .fold(menu, |menu, (label, route)| {
                    let weak = weak.clone();
                    menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| {
                            if route == WorkflowRoute::Mermaid {
                                this.prepare_mermaid_preview(cx);
                            }
                            cx.emit(WorkflowWorkspaceEvent::Navigate(route));
                        })
                        .ok();
                    }))
                })
            })
            .into_any_element()
    }

    fn render_workflow_properties(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .debug_selector(|| "workflow-properties".into())
            .gap_3()
            .pb_4()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Workflow"))
                    .child(
                        div().debug_selector(|| "workflow-enabled".into()).child(
                            Switch::new("workflow-enabled")
                                .checked(self.draft.definition.enabled)
                                .label(if self.draft.definition.enabled {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                })
                                .accessibility_label("Workflow enabled")
                                .small()
                                .on_change({
                                    let weak = cx.entity().downgrade();
                                    move |enabled, _, cx| {
                                        weak.update(cx, |this, cx| {
                                            if this.draft.definition.enabled != *enabled {
                                                this.toggle_workflow_enabled(cx);
                                            }
                                        })
                                        .ok();
                                    }
                                }),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child("Name"),
                    )
                    .child(
                        div()
                            .debug_selector(|| "workflow-name-field".into())
                            .min_w_0()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                Input::new(&self.draft.name_input)
                                    .aria_label("Workflow name")
                                    .appearance(false)
                                    .w_full(),
                            ),
                    ),
            )
            .child(
                h_flex().justify_end().child(
                    Button::new("workflow-discard")
                        .debug_selector(|| "workflow-discard".into())
                        .label("Discard changes")
                        .text()
                        .small()
                        .disabled(self.draft.saving || !self.draft.dirty(cx))
                        .on_click(cx.listener(|this, _, window, cx| this.discard(window, cx))),
                ),
            )
            .into_any_element()
    }

    fn render_builder_sidebar(&self, full: bool, cx: &mut Context<Self>) -> AnyElement {
        let selected_node = self.draft.selected_node.clone();
        let content = if selected_node.is_some() {
            v_flex()
                .gap_4()
                .child(self.render_selected_node_controls(selected_node, cx))
                .child(self.render_connections(cx))
                .child(
                    h_flex()
                        .pt_3()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            Button::new("workflow-remove-step")
                                .debug_selector(|| "workflow-remove-step".into())
                                .label("Remove step")
                                .danger()
                                .outline()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_selected_workflow_node(cx);
                                    if full {
                                        cx.emit(WorkflowWorkspaceEvent::Back);
                                    }
                                })),
                        ),
                )
                .into_any_element()
        } else {
            self.render_builder_intro(cx)
        };
        div()
            .id("workflow-inspector-scroll")
            .debug_selector(|| "workflow-inspector-scroll".into())
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .when(full, |this| this.flex_1().w_full())
            .when(!full, |this| {
                this.w(px(320.))
                    .border_l_1()
                    .border_color(cx.theme().border)
            })
            .child(
                v_flex().size_full().overflow_y_scrollbar().child(
                    v_flex()
                        .id("workflow-builder-sidebar")
                        .debug_selector(|| "workflow-builder-sidebar".into())
                        .p_4()
                        .gap_4()
                        .child(self.render_workflow_properties(cx))
                        .child(
                            h_flex()
                                .justify_between()
                                .child(div().font_weight(FontWeight::SEMIBOLD).child("Steps"))
                                .child(self.render_add_step(cx)),
                        )
                        .child(content),
                ),
            )
            .into_any_element()
    }

    fn render_builder_intro(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .id("workflow-builder-empty")
            .debug_selector(|| "workflow-builder-empty".into())
            .child(div().text_sm().text_color(theme.muted_foreground).child(
                if self.draft.definition.nodes.is_empty() {
                    "Start with a trigger, then add checks and actions as needed."
                } else {
                    "Select a step on the canvas to edit its behavior and outcomes."
                },
            ))
            .into_any_element()
    }

    fn render_connections(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(selected) = self.draft.selected_node.as_deref() else {
            return div().into_any_element();
        };
        let Some(node) = self
            .draft
            .definition
            .nodes
            .iter()
            .find(|node| node.id == selected)
        else {
            return div().into_any_element();
        };
        let outcomes = match &node.kind {
            WorkflowNodeKind::Condition { .. } => vec![
                ("Yes".to_string(), WorkflowEdgeKind::True),
                ("No".to_string(), WorkflowEdgeKind::False),
            ],
            WorkflowNodeKind::Branch { cases } => cases
                .iter()
                .map(|case| (case.label.clone(), WorkflowEdgeKind::Case(case.id.clone())))
                .chain(std::iter::once((
                    "Otherwise".into(),
                    WorkflowEdgeKind::Otherwise,
                )))
                .collect(),
            WorkflowNodeKind::End { .. } => Vec::new(),
            _ => vec![("Next".into(), WorkflowEdgeKind::Default)],
        };
        let rows = outcomes.into_iter().map(|(label, kind)| {
            let current = self
                .draft
                .definition
                .edges
                .iter()
                .find(|edge| edge.from == selected && edge.kind == kind);
            let value = current
                .and_then(|edge| {
                    self.draft
                        .definition
                        .nodes
                        .iter()
                        .find(|node| node.id == edge.to)
                })
                .map(|node| self.step_label(&node.kind))
                .unwrap_or_else(|| "Choose next step".into());
            let weak = cx.entity().downgrade();
            let source = selected.to_string();
            let targets = self
                .draft
                .definition
                .nodes
                .iter()
                .filter(|node| node.id != selected)
                .map(|node| {
                    (
                        node.id.clone(),
                        format!(
                            "{} · {}",
                            node_kind_label(&node.kind),
                            self.step_label(&node.kind)
                        ),
                    )
                })
                .collect::<Vec<_>>();
            let button = Button::new(SharedString::from(format!(
                "workflow-outcome-{selected}-{kind:?}"
            )))
            .debug_selector(|| "workflow-outcome-picker".into())
            .label(value)
            .outline()
            .dropdown_menu(move |menu, _, _| {
                let weak_remove = weak.clone();
                let from = source.clone();
                let outcome = kind.clone();
                let menu = menu.item(PopupMenuItem::new("Disconnect").on_click(move |_, _, cx| {
                    weak_remove
                        .update(cx, |this, cx| {
                            this.draft
                                .definition
                                .edges
                                .retain(|edge| !(edge.from == from && edge.kind == outcome));
                            cx.notify();
                        })
                        .ok();
                }));
                targets.iter().fold(menu, |menu, (id, label)| {
                    let weak = weak.clone();
                    let id = id.clone();
                    let kind = kind.clone();
                    let source = source.clone();
                    menu.item(PopupMenuItem::new(label.clone()).on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| {
                            this.draft
                                .definition
                                .edges
                                .retain(|edge| !(edge.from == source && edge.kind == kind));
                            this.connect_selected_workflow_node_to(id.clone(), kind.clone(), cx);
                        })
                        .ok();
                    }))
                })
            });
            v_flex()
                .gap_1()
                .child(div().text_sm().child(label))
                .child(button)
                .into_any_element()
        });
        v_flex()
            .gap_3()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Connections"))
            .children(rows)
            .into_any_element()
    }

    fn render_manual_run(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex().w_full().max_w(px(640.)).mx_auto().p_6().gap_4()
            .child("Choose an item to run this board’s saved manual workflows. Unsaved edits are not used.")
            .child(Select::new(&self.state.manual_entry_select).id("workflow-run-item").placeholder("Choose a board item").search_placeholder("Search items by title or list").w_full())
            .when_some(self.state.run_notice.clone(), |this, notice| {
                this.child(div().text_sm().text_color(cx.theme().muted_foreground).child(notice))
            })
            .child(Button::new("workflow-run-manual").label(if self.state.running {"Running…"} else {"Run saved workflows"}).primary().disabled(self.state.running)
                .on_click(cx.listener(|this,_,_,cx|this.run_manual_workflows_from_editor(cx))))
            .into_any_element()
    }
}

impl WorkflowWorkspace {
    fn workflow_summary(&self, definition: &WorkflowDefinition) -> String {
        definition
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    WorkflowNodeKind::Trigger { .. } | WorkflowNodeKind::Action { .. }
                )
            })
            .map(|node| {
                format!(
                    "{} {}",
                    node_kind_label(&node.kind),
                    self.step_label(&node.kind)
                )
            })
            .collect::<Vec<_>>()
            .join(" → ")
    }
}
