#![allow(clippy::expect_used, reason = "test fixtures must fail loudly")]
//! Fail-closed coverage for the portable conformance harness.

/// Failure-injection adapters for repository conformance tests.
pub mod conformance_failure_support;

#[path = "conformance_failures/lifecycle.rs"]
mod lifecycle_failures;
#[path = "conformance_failures/yards.rs"]
mod yard_failures;

use blobyard_contract::{MetadataRepository, RepositoryError};
use conformance_failure_support::{
    Corrupting, Corruption, Faulting, every_operation_fails_closed, repository, yard_fixture,
};

#[test]
fn metadata_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&Faulting::new(&repository, failure_index))
    });
}

#[test]
fn credential_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository)?;
        let workspace_id = repository.list_workspaces()?[0].id.clone();
        blobyard_testkit::credential_conformance(
            &Faulting::new(&repository, failure_index),
            &workspace_id,
        )
    });
}

#[test]
fn transfer_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository)?;
        blobyard_testkit::transfer_conformance(
            &Faulting::new(&repository, failure_index),
            "project_fixture",
        )
    });
}

#[test]
fn sharing_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository)?;
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")?;
        blobyard_testkit::sharing_conformance(&Faulting::new(&repository, failure_index))
    });
}

#[test]
fn inbox_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository)?;
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")?;
        blobyard_testkit::inbox_conformance(&Faulting::new(&repository, failure_index))
    });
}

#[test]
fn preview_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository)?;
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")?;
        blobyard_testkit::preview_conformance(&Faulting::new(&repository, failure_index))
    });
}

#[test]
fn lifecycle_conformance_propagates_each_adapter_failure() {
    every_operation_fails_closed(|failure_index| {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository)?;
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")?;
        blobyard_testkit::lifecycle_conformance(&Faulting::new(&repository, failure_index))
    });
}

#[test]
fn metadata_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::SchemaVersion,
        Corruption::WorkspaceList,
        Corruption::WorkspaceRecord,
        Corruption::RenamedWorkspaceList,
        Corruption::RenamedWorkspaceRecord,
        Corruption::ProjectList,
        Corruption::ProjectRecord,
        Corruption::CompleteState,
        Corruption::CompleteSize,
        Corruption::CompleteChecksum,
        Corruption::AbortedState,
    ] {
        let (_temporary, repository) = repository();
        let expected = if matches!(corruption, Corruption::SchemaVersion) {
            RepositoryError::SchemaTooNew
        } else {
            RepositoryError::Unavailable
        };
        assert_eq!(
            blobyard_testkit::repository_conformance(&Corrupting::new(&repository, corruption)),
            Err(expected),
            "{corruption:?}"
        );
    }
}

#[test]
fn credential_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::BootstrapFirstFalse,
        Corruption::BootstrapSecondTrue,
        Corruption::ActiveTokenRecord,
        Corruption::CreatedTokenRecord,
        Corruption::MonotonicTokenRecord,
        Corruption::CliSessionList,
        Corruption::FinalTokenListError,
        Corruption::FinalTokenListMismatch,
        Corruption::TokenList,
    ] {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        let workspace_id = repository.list_workspaces().expect("workspaces")[0]
            .id
            .clone();
        assert_eq!(
            blobyard_testkit::credential_conformance(
                &Corrupting::new(&repository, corruption),
                &workspace_id,
            ),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}

#[test]
fn transfer_conformance_rejects_each_inconsistent_record() {
    let corruptions = [
        Corruption::DownloadList,
        Corruption::FirstReservationVersion,
        Corruption::FirstReservationState,
        Corruption::FirstCapabilityRecord,
        Corruption::RenewedExpiry,
        Corruption::CompletedState,
        Corruption::CompletedVersion,
        Corruption::SecondReservationVersion,
        Corruption::LatestLength,
        Corruption::LatestVersion,
        Corruption::AllLength,
        Corruption::AllFirstVersion,
        Corruption::AllSecondVersion,
        Corruption::RequestedAbortPrior,
        Corruption::RequestedAbortStored,
        Corruption::UploadedAbortPrior,
        Corruption::UploadedAbortStored,
        Corruption::DownloadVersion,
        Corruption::MultipartReservation,
        Corruption::MultipartAttachment,
        Corruption::MultipartIssued,
        Corruption::MultipartResolution,
        Corruption::MultipartListing,
        Corruption::MultipartCompletion,
        Corruption::MultipartAbort,
    ];
    for corruption in corruptions {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        assert_eq!(
            blobyard_testkit::transfer_conformance(
                &Corrupting::new(&repository, corruption),
                "project_fixture",
            ),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}

#[test]
fn sharing_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::ShareCreatedRecord,
        Corruption::ShareList,
        Corruption::ShareResolvedTarget,
        Corruption::ShareIssuedTarget,
        Corruption::ShareFirstRevoke,
        Corruption::ShareSecondRevoke,
        Corruption::ShareFinalRecord,
        Corruption::ShareFinalList,
    ] {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")
            .expect("transfer conformance");
        assert_eq!(
            blobyard_testkit::sharing_conformance(&Corrupting::new(&repository, corruption)),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}

#[test]
fn inbox_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::InboxCreatedRecord,
        Corruption::InboxList,
        Corruption::InboxResolvedRecord,
        Corruption::InboxRateAllowed,
        Corruption::InboxRateLimited,
        Corruption::InboxRateReset,
        Corruption::InboxReservedRecord,
        Corruption::InboxReservedList,
        Corruption::InboxReservedCounters,
        Corruption::InboxCompletedRecord,
        Corruption::InboxCompletedList,
        Corruption::InboxCompletedCounters,
        Corruption::InboxAbortPrior,
        Corruption::InboxAbortStored,
        Corruption::InboxCapacityResult,
        Corruption::InboxReleasedList,
        Corruption::InboxReleasedCounters,
        Corruption::InboxExpiryResult,
        Corruption::InboxFirstRevoke,
        Corruption::InboxSecondRevoke,
        Corruption::InboxRevokedResolve,
    ] {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")
            .expect("transfer conformance");
        assert_eq!(
            blobyard_testkit::inbox_conformance(&Corrupting::new(&repository, corruption)),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}

#[test]
fn preview_conformance_rejects_each_inconsistent_record() {
    for corruption in [
        Corruption::PreviewInitialList,
        Corruption::PreviewObjectList,
        Corruption::PreviewCreatedRecord,
        Corruption::PreviewResolvedTarget,
        Corruption::PreviewList,
        Corruption::PreviewMissingResolution,
        Corruption::PreviewExpiredResolution,
        Corruption::PreviewFirstRevoke,
        Corruption::PreviewSecondRevoke,
        Corruption::PreviewRevokedResolution,
    ] {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")
            .expect("transfer conformance");
        assert_eq!(
            blobyard_testkit::preview_conformance(&Corrupting::new(&repository, corruption)),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}

#[test]
fn lifecycle_conformance_rejects_each_inconsistent_record() {
    let corruptions = [
        Corruption::DeletionComplete,
        Corruption::DeletionItems,
        Corruption::DeletionReplayIncomplete,
        Corruption::RetentionPolicy,
        Corruption::ClearFalse,
        Corruption::RetentionStatus,
        Corruption::AuditPageLength,
        Corruption::AuditCursor,
        Corruption::AuditNextLength,
        Corruption::AuditNextAction,
    ];
    for corruption in corruptions {
        let (_temporary, repository) = repository();
        blobyard_testkit::repository_conformance(&repository).expect("metadata conformance");
        blobyard_testkit::transfer_conformance(&repository, "project_fixture")
            .expect("transfer conformance");
        assert_eq!(
            blobyard_testkit::lifecycle_conformance(&Corrupting::new(&repository, corruption)),
            Err(RepositoryError::Unavailable),
            "{corruption:?}"
        );
    }
}
