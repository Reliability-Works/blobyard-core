use blobyard_contract::RetentionPolicyRecord;

pub(super) fn policy() -> RetentionPolicyRecord {
    RetentionPolicyRecord {
        project_id: "project_fixture".to_owned(),
        keep_latest: 1,
        path_glob: None,
        branch_glob: None,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}
