use super::*;

fn part_record() -> blobyard_contract::UploadPartRecord {
    blobyard_contract::UploadPartRecord {
        upload_id: "upload_fixture".to_owned(),
        part_number: 1,
        expected_size: 4,
        expires_at_ms: 10,
        received_size: None,
        received_checksum: None,
        provider_tag: None,
    }
}

#[tokio::test]
async fn receive_part_requires_provider_and_maps_storage_and_task_failures() {
    let root = TempDir::new().expect("root");
    let staging = root.path().join("staging");
    std::fs::create_dir(&staging).expect("staging");
    let ordinary = state(&root, staging.clone());
    assert_eq!(
        receive_part(
            &ordinary,
            &reservation("valid/key", &"00".repeat(32), 4),
            &part_record(),
            Body::from("data"),
        )
        .await
        .expect_err("missing provider")
        .into_response()
        .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    for (part, panic_on_part) in [
        (Err(StorageError::Unavailable), false),
        (
            Ok(MultipartPart {
                number: 1,
                size: 4,
                checksum: checksum(),
                provider_tag: None,
            }),
            true,
        ),
    ] {
        let storage = Arc::new(FixtureStorage {
            put: Err(StorageError::Unavailable),
            head: Err(StorageError::Unavailable),
            panic_on_put: false,
            part,
            panic_on_part,
        });
        let state = crate::test_support::state(&root, staging.clone(), storage);
        let mut upload = reservation("valid/key", &"00".repeat(32), 4);
        upload.provider_upload_id = Some("provider".to_owned());
        assert_eq!(
            receive_part(&state, &upload, &part_record(), Body::from("data"))
                .await
                .expect_err("part storage failure")
                .into_response()
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

#[test]
fn part_storage_maps_a_removed_staged_file() {
    let temporary = NamedTempFile::new().expect("temporary");
    std::fs::remove_file(temporary.path()).expect("remove temporary");
    let storage = fixture_storage(Ok(metadata(4)), Ok(metadata(4)));
    assert_eq!(
        store_part(&storage, &MultipartId("provider".to_owned()), 1, &temporary),
        Err(StorageError::Unavailable)
    );
}
