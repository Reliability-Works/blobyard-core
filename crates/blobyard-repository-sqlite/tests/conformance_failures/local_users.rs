use super::{Corrupting, Corruption, Faulting, every_operation_fails_closed, repository};
use blobyard_contract::{MetadataRepository, RepositoryError};

#[test]
fn local_user_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository)?;
        let workspace_id = repository.list_workspaces()?[0].id.clone();
        blobyard_testkit::local_user_conformance(
            &Faulting::new(&repository, failure_index),
            &workspace_id,
        )
    });
}

#[test]
fn local_user_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::LocalUserInitialList,
        Corruption::LocalUserFreshAuthentication,
        Corruption::LocalUserBoundaryAuthentication,
        Corruption::LocalUserResetAuthentication,
    ] {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        let workspace_id = repository.list_workspaces().expect("workspaces")[0]
            .id
            .clone();
        assert_eq!(
            blobyard_testkit::local_user_conformance(
                &Corrupting::new(&repository, corruption),
                &workspace_id,
            ),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}
