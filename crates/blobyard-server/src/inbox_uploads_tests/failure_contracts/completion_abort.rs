use super::*;

async fn uploaded_single(
    fixture: &test_seams::TransferFixture,
    idempotency: &str,
) -> (InboxGuest, String) {
    let guest = inbox_guest(fixture).await;
    let now = guest.inbox.created_at_ms + 1;
    let _ = issue_at(
        &fixture.state,
        &guest,
        &upload_request("complete.txt", 1),
        idempotency,
        now,
    )
    .expect("grant");
    let id = upload_id(&guest, idempotency);
    fixture
        .state
        .repository
        .record_uploaded_bytes(&id, 1, &hash("x"))
        .expect("uploaded bytes");
    (guest, id)
}

#[tokio::test]
async fn inbox_completion_checks_every_dependency_before_committing() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let guest = inbox_guest(&fixture).await;
    let missing = CompleteUploadRequest {
        upload_id: "missing".to_owned(),
        parts: Vec::new(),
    };
    assert_eq!(
        status(complete_at(&fixture.state, &guest, &missing, 1)),
        StatusCode::NOT_FOUND
    );

    let fixture = test_seams::fixture(&["inbox:manage"]);
    let (guest, id) = uploaded_single(&fixture, "invalid-parts").await;
    let invalid_parts = CompleteUploadRequest {
        upload_id: id,
        parts: vec![CompletedPart {
            part_number: 1,
            etag: "etag".to_owned(),
        }],
    };
    assert_eq!(
        status(complete_at(
            &fixture.state,
            &guest,
            &invalid_parts,
            guest.inbox.created_at_ms + 2,
        )),
        StatusCode::BAD_REQUEST
    );

    for failure_index in 0..=3 {
        let fixture = test_seams::fixture(&["inbox:manage"]);
        let (guest, id) = uploaded_single(&fixture, &format!("complete-{failure_index}")).await;
        let complete = CompleteUploadRequest {
            upload_id: id,
            parts: Vec::new(),
        };
        assert_eq!(
            status(complete_at(
                &faulted(&fixture.state, failure_index),
                &guest,
                &complete,
                guest.inbox.created_at_ms + 2,
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[tokio::test]
async fn inbox_abort_preserves_reservation_when_storage_or_repository_is_unavailable() {
    let fixture = test_seams::fixture(&["inbox:manage"]);
    let guest = inbox_guest(&fixture).await;
    let missing = AbortUploadRequest {
        upload_id: "missing".to_owned(),
    };
    assert_eq!(
        status(abort_at(&fixture.state, &guest, &missing, 1)),
        StatusCode::NOT_FOUND
    );

    let fixture = test_seams::fixture(&["inbox:manage"]);
    let guest = inbox_guest(&fixture).await;
    let now = guest.inbox.created_at_ms + 1;
    let _ = issue_at(
        &fixture.state,
        &guest,
        &upload_request("abort.txt", 1),
        "abort-storage",
        now,
    )
    .expect("grant");
    let abort = AbortUploadRequest {
        upload_id: upload_id(&guest, "abort-storage"),
    };
    let mut unavailable_storage = fixture.state.clone();
    unavailable_storage.storage = Arc::new(MultipartStorage::unavailable());
    assert_eq!(
        status(abort_at(&unavailable_storage, &guest, &abort, now + 1,)),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    for failure_index in 0..=1 {
        let fixture = test_seams::fixture(&["inbox:manage"]);
        let guest = inbox_guest(&fixture).await;
        let now = guest.inbox.created_at_ms + 1;
        let idempotency = format!("abort-{failure_index}");
        let _ = issue_at(
            &fixture.state,
            &guest,
            &upload_request("abort.txt", 1),
            &idempotency,
            now,
        )
        .expect("grant");
        let abort = AbortUploadRequest {
            upload_id: upload_id(&guest, &idempotency),
        };
        assert_eq!(
            status(abort_at(
                &faulted(&fixture.state, failure_index),
                &guest,
                &abort,
                now + 1,
            )),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
