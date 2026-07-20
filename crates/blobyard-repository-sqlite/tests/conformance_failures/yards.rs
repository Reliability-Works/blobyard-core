use super::{
    Corrupting, Corruption, Faulting, RepositoryError, every_operation_fails_closed, repository,
    yard_fixture,
};

fn seed(repository: &blobyard_repository_sqlite::SqliteRepository) {
    blobyard_testkit::repository_conformance(repository).expect("metadata conformance");
    blobyard_testkit::transfer_conformance(repository, "project_fixture")
        .expect("transfer conformance");
}

#[test]
fn yard_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        seed(&repository);
        blobyard_testkit::yard_conformance(
            &Faulting::new(&repository, failure_index),
            &yard_fixture(),
        )
    });
}

#[test]
fn yard_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::YardInitialList,
        Corruption::YardFixtureObjectList,
        Corruption::YardReusedStart,
        Corruption::YardReplacementStatus,
        Corruption::YardDelayedStatus,
        Corruption::YardRollbackRecord,
        Corruption::YardFailureRecord,
        Corruption::YardListShape,
        Corruption::YardDeliveryTarget,
        Corruption::YardFirstDelete,
        Corruption::YardSecondDelete,
        Corruption::YardFinalRecord,
        Corruption::YardDeletedResolution,
    ] {
        let (_temporary, repository) = repository();
        seed(&repository);
        assert_eq!(
            blobyard_testkit::yard_conformance(
                &Corrupting::new(&repository, corruption),
                &yard_fixture(),
            ),
            Err(RepositoryError::Unavailable),
            "corruption {corruption:?} must fail closed"
        );
    }
}
