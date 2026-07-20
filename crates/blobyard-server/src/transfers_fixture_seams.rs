use super::{
    ApiError, Body, IntoResponse, IssueFailure, ProjectRecord, RepositoryError, Response,
    SecretString, Slug, TransferFixture, WorkspaceRecord, upload_request,
};

impl TransferFixture {
    /// Runs one deterministic issuance failure and verifies that it writes no reservation or audit.
    #[must_use]
    pub fn issue_failure(&self, failure: IssueFailure) -> Response {
        let mut state = self.state.clone();
        let now = match failure {
            IssueFailure::Clock => Err(ApiError::internal()),
            IssueFailure::ExpiryOverflow => Ok(u64::MAX),
            IssueFailure::TransferUrl => {
                "invalid\norigin".clone_into(&mut state.public_origin);
                Ok(0)
            }
        };
        let result = super::super::issue_upload_at(
            &state,
            &self.principal,
            &upload_request(),
            &self.project,
            "failure-fixture",
            blobyard_contract::ObjectSource::Cli,
            now,
        );
        let upload_id =
            crate::transfer_grants::stable_upload_id(&self.principal.id, "failure-fixture");
        assert_eq!(
            self.state.repository.upload_by_id(&upload_id),
            Err(RepositoryError::NotFound)
        );
        assert!(
            self.state
                .repository
                .list_audit(&self.principal.workspace_id, None, 1)
                .expect("audit query")
                .items
                .is_empty()
        );
        result.err().expect("issuance failure").into_response()
    }

    /// Exercises a clock failure before capability lookup or body staging.
    pub async fn put_clock_failure(&self) -> Response {
        let capability = SecretString::new("valid-capability").expect("capability");
        super::super::put_upload_at(
            &self.state,
            &capability,
            Body::empty(),
            Err(ApiError::internal()),
        )
        .await
        .expect_err("clock failure")
        .into_response()
    }

    /// Seeds a valid reservation in a different workspace to exercise ownership concealment.
    #[must_use]
    pub fn seed_foreign_upload(&self) -> String {
        let workspace = WorkspaceRecord {
            id: "workspace_foreign".to_owned(),
            name: "Foreign".to_owned(),
            slug: Slug::new("foreign").expect("workspace slug"),
        };
        self.state
            .repository
            .create_workspace(&workspace)
            .expect("foreign workspace");
        let project = ProjectRecord {
            id: "project_foreign".to_owned(),
            workspace_id: workspace.id,
            name: "Foreign".to_owned(),
            slug: Slug::new("foreign").expect("project slug"),
        };
        self.state
            .repository
            .create_project(&project)
            .expect("foreign project");
        let upload_id = "upload_foreign".to_owned();
        let capability = SecretString::new("foreign-capability").expect("capability");
        let input = crate::transfer_grants::reservation_input(
            &upload_request(),
            &project,
            &upload_id,
            &capability,
            u64::try_from(i64::MAX).expect("expiry"),
            blobyard_contract::ObjectSource::Cli,
        );
        self.state
            .repository
            .reserve_upload(&input)
            .expect("foreign reservation");
        upload_id
    }

    /// Makes the object path a directory so filesystem deletion fails deterministically.
    pub fn block_storage_delete(&self, upload_id: &str) {
        let reservation = self
            .state
            .repository
            .upload_by_id(upload_id)
            .expect("reservation");
        std::fs::create_dir_all(
            self.root
                .path()
                .join("objects/objects")
                .join(reservation.version.storage_key),
        )
        .expect("storage blocker");
    }

    /// Verifies that a rejected ownership check did not change reservation state.
    #[must_use]
    pub fn is_requested(&self, upload_id: &str) -> bool {
        self.state
            .repository
            .upload_by_id(upload_id)
            .is_ok_and(|reservation| {
                reservation.state == blobyard_contract::ReservationState::Requested
            })
    }

    /// Returns durable namespace counts for no-mutation assertions.
    #[must_use]
    pub fn namespace_counts(&self) -> (usize, usize) {
        let workspaces = self.state.repository.list_workspaces().expect("workspaces");
        let project_count = workspaces
            .iter()
            .map(|workspace| {
                self.state
                    .repository
                    .list_projects(&workspace.id)
                    .expect("projects")
                    .len()
            })
            .sum();
        (workspaces.len(), project_count)
    }
}
