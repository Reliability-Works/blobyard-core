use super::*;

#[test]
fn authentication_rejects_a_machine_token_corrupted_after_minting() {
    let fixture = crate::transfers::test_seams::fixture(&["object:read"]);
    crate::test_support::install_machine_session(&fixture, "machine-secret", "auth_fixture", 10);
    fixture
        .state
        .repository
        .create_project(&ProjectRecord {
            id: "project_other".to_owned(),
            workspace_id: fixture.principal.workspace_id.clone(),
            name: "Other".to_owned(),
            slug: Slug::new("other").expect("other project slug"),
        })
        .expect("other project");
    fixture.corrupt_machine_project("machine-secret");
    let error = super::super::test_seams::authenticate_at(&fixture.state, "machine-secret", 11)
        .expect_err("corrupted machine record");
    assert_eq!(error.into_response().status(), StatusCode::UNAUTHORIZED);
}
