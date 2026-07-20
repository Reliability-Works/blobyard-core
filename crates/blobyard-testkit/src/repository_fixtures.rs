use blobyard_contract::{NewUploadReservation, ObjectSource, RepositoryError, ReservationStrategy};
use blobyard_core::Slug;

pub(super) struct NamespaceFixtures {
    pub(super) workspace: &'static str,
    pub(super) renamed_workspace: &'static str,
    pub(super) project: &'static str,
}

impl NamespaceFixtures {
    pub(super) const fn valid() -> Self {
        Self {
            workspace: "fixture",
            renamed_workspace: "renamed",
            project: "project",
        }
    }

    pub(super) fn validate(&self) -> Result<ValidatedNamespaceFixtures, RepositoryError> {
        Ok(ValidatedNamespaceFixtures {
            workspace: Slug::new(self.workspace.to_owned())
                .map_err(|_error| RepositoryError::Unavailable)?,
            renamed_workspace: Slug::new(self.renamed_workspace.to_owned())
                .map_err(|_error| RepositoryError::Unavailable)?,
            project: Slug::new(self.project.to_owned())
                .map_err(|_error| RepositoryError::Unavailable)?,
        })
    }
}

pub(super) struct ValidatedNamespaceFixtures {
    pub(super) workspace: Slug,
    pub(super) renamed_workspace: Slug,
    pub(super) project: Slug,
}

pub(super) const fn hello_checksum() -> &'static str {
    "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
}

pub(super) fn hash(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

pub(super) fn upload(
    id: &str,
    project_id: &str,
    path: &str,
    hash_character: char,
) -> NewUploadReservation {
    NewUploadReservation {
        id: id.to_owned(),
        project_id: project_id.to_owned(),
        object_path: path.to_owned(),
        filename: "build.zip".to_owned(),
        content_type: "application/zip".to_owned(),
        expected_size: 5,
        expected_checksum: hello_checksum().to_owned(),
        storage_key: format!("objects/{id}"),
        capability_hash: hash(hash_character),
        expires_at_ms: 1_000,
        created_at_ms: 500,
        source: ObjectSource::Ci,
        git_repository: Some("Reliability-Works/blobyard-core".to_owned()),
        git_commit: Some("0123456789abcdef".to_owned()),
        git_branch: Some("main".to_owned()),
        strategy: ReservationStrategy::Single,
        part_size: None,
        part_count: None,
    }
}
