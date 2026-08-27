use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use entity::{note, note::Entity as Note};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, TransactionTrait};

use super::folder_import::{MAX_FILE_BYTES, has_supported_extension};

#[derive(Debug)]
pub struct FileImport {
    path: PathBuf,
    title: String,
    content: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FileImportResult {
    pub note_id: u32,
    pub project_id: Option<u32>,
    pub title: String,
}

pub fn scan_file(path: &Path) -> Result<FileImport> {
    let path = path
        .canonicalize()
        .with_context(|| format!("Could not resolve {}", path.display()))?;

    if !path.is_file() {
        bail!("{} is not a file", path.display());
    }
    if !has_supported_extension(&path) {
        bail!(
            "{} is not a supported Markdown, JSON, or plain text file",
            path.display()
        );
    }

    let file_bytes = fs::metadata(&path)?.len();
    if file_bytes > MAX_FILE_BYTES {
        bail!("{} is larger than the 2 MiB import limit", path.display());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Could not read {} as UTF-8 text", path.display()))?;
    let title = path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Untitled document")
        .to_string();

    Ok(FileImport {
        path,
        title,
        content,
    })
}

pub async fn import_file(
    db: &(impl ConnectionTrait + TransactionTrait),
    file: FileImport,
) -> Result<FileImportResult> {
    let file_path = file.path.to_string_lossy().into_owned();
    let existing = Note::find()
        .filter(note::Column::FilePath.eq(file_path.clone()))
        .one(db)
        .await?;
    if existing
        .as_ref()
        .is_some_and(|note| note.deleted_at.is_some())
    {
        bail!("This file is already in Trash; restore it before importing it again");
    }
    let project_id = existing
        .and_then(|note| note.project_id)
        .map(u32::try_from)
        .transpose()
        .context("The imported file has an invalid project id")?;

    let note = super::import_external_note(db, file.title, file_path, file.content).await?;
    Ok(FileImportResult {
        note_id: note.id,
        project_id,
        title: note.title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{Database, EntityTrait};

    #[test]
    fn scan_file_validates_supported_text_and_uses_an_extensionless_title() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("archive.v1.MARKDOWN");
        fs::write(&path, "# Archive")?;

        let file = scan_file(&path)?;

        assert_eq!(file.path, path.canonicalize()?);
        assert_eq!(file.title, "archive.v1");
        assert_eq!(file.content, "# Archive");

        let unsupported = directory.path().join("image.png");
        fs::write(&unsupported, "not an image")?;
        assert!(scan_file(&unsupported).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn importing_the_same_file_refreshes_one_standalone_external_note() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("notes.md");
        fs::write(&path, "first")?;

        let first = import_file(&db, scan_file(&path)?).await?;
        fs::write(&path, "second [[Missing note]]")?;
        let second = import_file(&db, scan_file(&path)?).await?;

        assert_eq!(first.note_id, second.note_id);
        assert_eq!(first.project_id, None);
        assert_eq!(first.title, "notes");
        let notes = Note::find().all(&db).await?;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].project_id, None);
        assert_eq!(notes[0].cached_content, "second [[Missing note]]");
        assert!(!notes[0].file_managed_by_app);
        assert_eq!(
            notes[0].file_path.as_deref(),
            Some(path.canonicalize()?.to_string_lossy().as_ref())
        );
        let links = crate::note::links::load_note_links(&db, notes[0].id).await?;
        assert_eq!(links.unresolved.len(), 1);
        assert_eq!(links.unresolved[0].raw_target, "Missing note");
        assert_eq!(notes[0].file_missing_since, None);
        Ok(())
    }

    #[tokio::test]
    async fn importing_a_file_already_in_a_folder_project_preserves_its_project() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("project-note.md");
        fs::write(&path, "folder import")?;

        let folder = super::super::folder_import::import_folder(
            &db,
            super::super::folder_import::scan_folder(directory.path())?,
        )
        .await?;
        assert!(folder.created_project);

        fs::write(&path, "individual refresh")?;
        let imported = import_file(&db, scan_file(&path)?).await?;
        let notes = Note::find().all(&db).await?;

        assert_eq!(notes.len(), 1);
        assert_eq!(imported.project_id, notes[0].project_id.map(|id| id as u32));
        assert_eq!(notes[0].cached_content, "individual refresh");
        Ok(())
    }
}
