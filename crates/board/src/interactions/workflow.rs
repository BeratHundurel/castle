use super::*;

impl BoardView {
    pub(crate) fn set_list_workflow_role(
        &mut self,
        list_id: u32,
        workflow_role: storage::board::ListWorkflowRole,
        cx: &mut Context<Self>,
    ) {
        let Some(list) = self.data.lists.iter_mut().find(|list| list.id == list_id) else {
            return;
        };
        if list.workflow_role == workflow_role {
            return;
        }

        list.workflow_role = workflow_role;
        cx.notify();

        self.commit_board_mutation(
            cx,
            "Could not update list workflow role",
            false,
            move |store| async move {
                storage::board::commands::set_board_list_workflow_role(
                    &store,
                    list_id,
                    workflow_role,
                )
                .await
            },
        );
    }

    pub(crate) fn run_move_workflow_after_layout(
        &self,
        board_id: u32,
        entry_id: u32,
        source_list_id: u32,
        target_list_id: u32,
        cx: &mut Context<Self>,
    ) {
        if source_list_id == target_list_id {
            return;
        }
        let Some(_source_list) = self
            .data
            .lists
            .iter()
            .find(|list| list.id == source_list_id)
        else {
            return;
        };
        let Some(_target_list) = self
            .data
            .lists
            .iter()
            .find(|list| list.id == target_list_id)
        else {
            return;
        };

        let app_runtime = cx.global::<AppRuntime>().clone();
        let store = app_runtime.store();
        let persistence = cx.global::<crate::BoardServices>().layout_persistence();
        let task = app_runtime.spawn_tokio(cx.background_executor(), async move {
            persistence.wait_for_pending(board_id).await?;
            storage::workflow::run_persisted_move_event(
                &store,
                i64::from(entry_id),
                i64::from(source_list_id),
                i64::from(target_list_id),
                ::workflow::EventOrigin::User,
            )
            .await
        });
        cx.spawn(async move |this, cx| match task.await {
            Ok(Ok(report)) if !report.runs.is_empty() => {
                this.update(cx, |this, cx| {
                    if this.data.board_id == Some(board_id) {
                        this.enrich_board_async(cx, board_id);
                    }
                })
                .ok();
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                this.update(cx, |this, cx| {
                    this.mutation.mutation_error =
                        Some(format!("Board workflow failed: {error}").into());
                    cx.notify();
                })
                .ok();
            }
            Err(error) => {
                this.update(cx, |this, cx| {
                    this.mutation.mutation_error =
                        Some(format!("Board workflow task failed: {error}").into());
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}
