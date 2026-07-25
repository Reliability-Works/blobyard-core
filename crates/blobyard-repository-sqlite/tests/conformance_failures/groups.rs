use super::{Corrupting, Corruption, RepositoryError, repository};
use blobyard_contract::MetadataRepository;

#[test]
fn group_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::GroupInitialList,
        Corruption::GroupMemberList,
        Corruption::GroupFinalCount,
        Corruption::GroupMissingUser,
        Corruption::GroupMissingFinal,
    ] {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        let workspace_id = repository.list_workspaces().expect("workspaces")[0]
            .id
            .clone();
        blobyard_testkit::local_user_conformance(&repository, &workspace_id)
            .expect("local user conformance");
        assert_eq!(
            blobyard_testkit::group_conformance(
                &Corrupting::new(&repository, corruption),
                &workspace_id,
            ),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}
