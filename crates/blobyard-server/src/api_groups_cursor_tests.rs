use super::{
    decode_group, decode_member, encode_group, encode_group_option, encode_member,
    encode_member_option,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use blobyard_contract::{WorkspaceGroupCursor, WorkspaceGroupMemberCursor};

#[test]
fn cursors_round_trip_and_are_bound_to_their_scope_and_shape() {
    let group = WorkspaceGroupCursor {
        created_at_ms: 42,
        id: "group_0123456789abcdef0123456789abcdef".to_owned(),
    };
    let encoded = encode_group("workspace_fixture", &group);
    assert_eq!(
        decode_group("workspace_fixture", Some(&encoded))
            .ok()
            .flatten(),
        Some(group.clone())
    );
    assert!(decode_group("workspace_other", Some(&encoded)).is_err());
    assert!(decode_group("workspace_fixture", Some("not-base64!")).is_err());
    assert!(encode_group_option("workspace_fixture", None).is_none());
    assert!(encode_group_option("workspace_fixture", Some(&group)).is_some());

    let member = WorkspaceGroupMemberCursor {
        added_at_ms: 43,
        user_id: "user_fixture".to_owned(),
    };
    let encoded = encode_member("group_fixture", &member);
    assert_eq!(
        decode_member("group_fixture", Some(&encoded))
            .ok()
            .flatten(),
        Some(member.clone())
    );
    assert!(decode_member("group_fixture", Some(&"x".repeat(1_025))).is_err());
    assert!(encode_member_option("group_fixture", None).is_none());
    assert!(encode_member_option("group_fixture", Some(&member)).is_some());
    for malformed in [b"{}".as_slice(), br#"{"scope":"group_fixture"}"#] {
        let encoded = URL_SAFE_NO_PAD.encode(malformed);
        assert!(decode_member("group_fixture", Some(&encoded)).is_err());
    }
}
