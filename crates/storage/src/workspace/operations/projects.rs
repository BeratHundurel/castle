use super::*;

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let projects = Project::find()
            .filter(project::Column::Archived.eq(false))
            .filter(project::Column::DeletedAt.is_null())
            .order_by_asc(project::Column::Position)
            .order_by_asc(project::Column::Id)
            .all(self.db.as_ref())
            .await?;

        let board_counts = Board::find()
            .filter(board::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .filter_map(|board| board.project_id)
            .fold(HashMap::<i64, u64>::new(), |mut counts, project_id| {
                *counts.entry(project_id).or_default() += 1;
                counts
            });

        Ok(projects
            .into_iter()
            .map(|project| ProjectSummary {
                id: project.id,
                name: project.name,
                position: project.position,
                board_count: board_counts.get(&project.id).copied().unwrap_or_default(),
            })
            .collect())
    }

    pub async fn create_project(&self, input: CreateProjectInput) -> Result<ProjectSummary> {
        let name = required_text(input.name, "project name")?;
        let project = crate::workspace::create_project(self, name).await?;
        Ok(ProjectSummary {
            id: i64::from(project.id),
            name: project.name,
            position: project.position,
            board_count: 0,
        })
    }

    pub async fn rename_project(&self, input: RenameProjectInput) -> Result<ProjectSummary> {
        self.active_project(input.project_id).await?;
        crate::workspace::rename_project(
            self,
            u32::try_from(input.project_id).context("project ID is out of range")?,
            required_text(input.name, "project name")?,
        )
        .await?;
        self.list_projects()
            .await?
            .into_iter()
            .find(|project| project.id == input.project_id)
            .with_context(|| format!("renamed project {} was not found", input.project_id))
    }
}
