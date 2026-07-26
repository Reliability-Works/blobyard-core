use super::{
    success,
    transaction_edges::support::{created, deploy, repository, yard},
};
use blobyard_contract::{NewWebYard, RepositoryError, WebYardRepository, YardVisibility};
use blobyard_testkit::{granted_event, new_grant, revoked_event, visibility_event, yard_event};

fn started(repository: &crate::adapter::SqliteRepository, name: &str, number: u64) -> NewWebYard {
    let candidate = yard(name, number);
    success(repository.start_yard_deploy(
        &candidate,
        &deploy(&candidate, number, false),
        &created(&candidate.id, number),
    ));
    crate::adapter::tests::approve_access_policy(
        repository,
        &candidate.id,
        "user_fixture",
        number + 100,
    );
    candidate
}

#[test]
fn access_mutations_conceal_unknown_and_deleted_yards() {
    let (_temporary, repository, _version, _size) = repository();
    let candidate = started(&repository, "docs", 1);
    success(repository.delete_web_yard(
        &candidate.id,
        2,
        &yard_event("yard.deleted", "web_yard", "yardId", &candidate.id, 2),
    ));
    for yard_id in ["yard_unknown", candidate.id.as_str()] {
        assert_eq!(
            repository.set_yard_visibility(
                yard_id,
                YardVisibility::Owner,
                3,
                &visibility_event(yard_id, "public", "owner", 3),
            ),
            Err(RepositoryError::NotFound)
        );
        let concealed = new_grant("grant_concealed", yard_id, None, None, 3);
        assert_eq!(
            repository.insert_yard_access_grant(&concealed, &granted_event(yard_id, &concealed, 3)),
            Err(RepositoryError::NotFound)
        );
        assert_eq!(
            repository.revoke_yard_access_grant(
                yard_id,
                "grant_concealed",
                3,
                &revoked_event(yard_id, "grant_concealed", 3),
            ),
            Err(RepositoryError::NotFound)
        );
    }
}

#[test]
fn grants_validate_roles_and_conceal_foreign_yards() {
    let (_temporary, repository, _version, _size) = repository();
    let docs = started(&repository, "docs", 1);
    let oversized = "role".repeat(17);
    let invalid_roles: [&[&str]; 4] = [
        &["viewer", "viewer"],
        &[oversized.as_str()],
        &["bad\u{1}"],
        &[" padded "],
    ];
    for roles in invalid_roles {
        let mut invalid = new_grant("grant_invalid", &docs.id, None, None, 3);
        invalid.app_roles = roles.iter().map(|role| (*role).to_owned()).collect();
        assert_eq!(
            repository.insert_yard_access_grant(&invalid, &granted_event(&docs.id, &invalid, 3)),
            Err(RepositoryError::InvalidInput)
        );
    }
    let mut crowded = new_grant("grant_invalid", &docs.id, None, None, 3);
    crowded.app_roles = (0..17).map(|index| format!("role{index}")).collect();
    let mut unnamed = new_grant("grant_invalid", &docs.id, None, None, 3);
    unnamed.id.clear();
    assert_eq!(
        repository.insert_yard_access_grant(&crowded, &granted_event(&docs.id, &crowded, 3)),
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        repository.insert_yard_access_grant(&unnamed, &granted_event(&docs.id, &unnamed, 3)),
        Err(RepositoryError::InvalidInput)
    );
}

#[test]
fn grants_validate_expiries_environments_and_audit_events() {
    let (_temporary, repository, _version, _size) = repository();
    let docs = started(&repository, "docs", 1);
    let blog = started(&repository, "blog", 2);
    let mut unbounded_expiry = new_grant("grant_invalid", &docs.id, None, Some(u64::MAX), 3);
    unbounded_expiry.created_at_ms = 3;
    let unnamed_environment = new_grant("grant_invalid", &docs.id, Some(""), None, 3);
    for invalid in [unbounded_expiry, unnamed_environment] {
        assert_eq!(
            repository.insert_yard_access_grant(&invalid, &granted_event(&docs.id, &invalid, 3)),
            Err(RepositoryError::InvalidInput)
        );
    }
    let granted = new_grant("grant_docs", &docs.id, None, None, 3);
    assert_eq!(
        repository.insert_yard_access_grant(&granted, &revoked_event(&docs.id, &granted.id, 3)),
        Err(RepositoryError::InvalidInput),
        "a mismatched grant audit event must be rejected"
    );
    success(repository.insert_yard_access_grant(&granted, &granted_event(&docs.id, &granted, 3)));
    assert_eq!(
        repository.revoke_yard_access_grant(
            &docs.id,
            "grant_docs",
            4,
            &granted_event(&docs.id, &granted, 4),
        ),
        Err(RepositoryError::InvalidInput),
        "a mismatched revocation audit event must be rejected"
    );
    assert_eq!(
        repository.revoke_yard_access_grant(
            &blog.id,
            "grant_docs",
            4,
            &revoked_event(&blog.id, "grant_docs", 4),
        ),
        Err(RepositoryError::NotFound)
    );
}

#[test]
fn visibility_round_trips_and_rejects_mismatched_audit_events() {
    let (_temporary, repository, _version, _size) = repository();
    let candidate = started(&repository, "docs", 1);
    assert_eq!(
        repository.set_yard_visibility(
            &candidate.id,
            YardVisibility::Workspace,
            2,
            &visibility_event(&candidate.id, "owner", "workspace", 2),
        ),
        Err(RepositoryError::InvalidInput)
    );
    let policy = success(repository.set_yard_visibility(
        &candidate.id,
        YardVisibility::Workspace,
        2,
        &visibility_event(&candidate.id, "public", "workspace", 2),
    ));
    assert_eq!(policy.visibility, YardVisibility::Workspace);
    assert_eq!(policy.updated_by_principal, "fixture");
    let read = success(repository.get_yard_access_policy(&candidate.id));
    assert_eq!(read, Some(policy));
}
