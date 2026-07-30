use super::{
    Corrupting, Corruption, Faulting, RepositoryError, every_operation_fails_closed, repository,
    yard_fixture,
};

const YARD_CORRUPTIONS: &[Corruption] = &[
    Corruption::YardInitialList,
    Corruption::YardEnvironmentList,
    Corruption::YardUnknownEnvironmentList,
    Corruption::YardPhantomPolicy,
    Corruption::YardPhantomGrantList,
    Corruption::YardUnknownGrantList,
    Corruption::YardVisibilityRecord,
    Corruption::YardRestoredVisibility,
    Corruption::YardPrivateDelivery,
    Corruption::YardGrantRecord,
    Corruption::YardScopedGrantRecord,
    Corruption::YardGrantValidation,
    Corruption::YardExpiredGrantList,
    Corruption::YardRevokedGrantList,
    Corruption::YardMissingGrantRevoke,
    Corruption::YardFirstRevoke,
    Corruption::YardSecondRevoke,
    Corruption::YardGrantListOrder,
    Corruption::YardAccessEnvironmentSeed,
    Corruption::YardSessionEnvironmentSeed,
    Corruption::YardSessionAdmission,
    Corruption::YardSessionExchange,
    Corruption::YardSessionMissingList,
    Corruption::YardSessionList,
    Corruption::YardDirectDeliveryTarget,
    Corruption::YardDirectSessionRevoke,
    Corruption::YardSessionLiveTarget,
    Corruption::YardSessionPublicTarget,
    Corruption::YardSessionRevocationList,
    Corruption::YardSessionFirstRevoke,
    Corruption::YardSessionLogoutRevoke,
    Corruption::YardSessionDeactivation,
    Corruption::YardGuestEnvironmentSeed,
    Corruption::YardGuestCreatedRecord,
    Corruption::YardGuestBoundaryScope,
    Corruption::YardGuestCapacityCreateFailure,
    Corruption::YardGuestCapacityOverflowAccepted,
    Corruption::YardGuestIdentityRecord,
    Corruption::YardOidcMemberBinding,
    Corruption::YardOidcReturningBinding,
    Corruption::YardOidcGuestBinding,
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
];

fn seed(repository: &blobyard_repository_sqlite::SqliteRepository) {
    blobyard_testkit::repository_conformance(repository).expect("metadata conformance");
    blobyard_testkit::transfer_conformance(repository, "project_fixture")
        .expect("transfer conformance");
}

#[test]
fn yard_conformance_propagates_each_adapter_failure() {
    let operation_count = every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        seed(&repository);
        blobyard_testkit::yard_fault_conformance(
            &Faulting::new(&repository, failure_index),
            &yard_fixture(),
        )
    });
    assert_eq!(
        operation_count, 258,
        "fault inventory must retain each canonical Yard boundary while omitting the repeated \
         100-invitation capacity fill"
    );
}

#[test]
fn yard_conformance_rejects_each_inconsistent_record() {
    for &corruption in YARD_CORRUPTIONS {
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
