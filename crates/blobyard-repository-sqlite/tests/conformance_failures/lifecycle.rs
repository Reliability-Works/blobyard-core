use super::{Faulting, RepositoryError, repository};
use blobyard_contract::LifecycleRepository;

#[test]
fn lifecycle_failure_adapter_forwards_the_remaining_operations() {
    let (_temporary, repository) = repository();
    blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
    let faulting = Faulting::new(&repository, usize::MAX);
    let mut event =
        blobyard_testkit::workspace_renamed_event("workspace_fixture", "previous-workspace", 2);
    event.id = "audit_forwarded".to_owned();
    event.request_id = "request_forwarded".to_owned();

    assert_eq!(faulting.retained_projects(), Ok(Vec::new()));
    assert_eq!(faulting.record_audit(&event), Ok(()));
    assert_eq!(
        faulting.fail_retention("missing", 1),
        Err(RepositoryError::NotFound)
    );
    assert_eq!(
        Faulting::new(&repository, 0).retained_projects(),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        Faulting::new(&repository, 0).record_audit(&event),
        Err(RepositoryError::Unavailable)
    );
    assert_eq!(
        Faulting::new(&repository, 0).fail_retention("missing", 1),
        Err(RepositoryError::Unavailable)
    );
}
