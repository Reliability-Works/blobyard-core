use super::support::{BUCKET, TestResult, event, object_uri, storage as replay_storage};
use blobyard_contract::{ObjectStorageInventory, StorageError, StorageKey};
use http::Method;

#[test]
fn inventory_paginates_strips_prefix_and_sorts() -> TestResult {
    let first = list_response(
        ["tenant/core/z-last", "tenant/core/nested/a"],
        true,
        Some("next-token"),
    );
    let second = list_response(["tenant/core/b-middle"], false, None);
    let events = vec![
        event(
            Method::GET,
            &list_uri(Some("tenant/core/"), None),
            "",
            200,
            first,
        )?,
        event(
            Method::GET,
            &list_uri(Some("tenant/core/"), Some("next-token")),
            "",
            200,
            second,
        )?,
    ];
    let (_temporary, storage, replay) = replay_storage(events, Some("tenant/core"))?;
    assert_eq!(
        storage
            .list_object_keys()?
            .iter()
            .map(StorageKey::as_str)
            .collect::<Vec<_>>(),
        ["b-middle", "nested/a", "z-last"]
    );
    replay.assert_requests_match(&[]);
    Ok(())
}

#[test]
fn inventory_rejects_invalid_pages_and_provider_keys() -> TestResult {
    let duplicate = list_response(["same", "same"], false, None);
    let (_temporary, storage, replay) = replay_storage(
        vec![event(
            Method::GET,
            &list_uri(None, None),
            "",
            200,
            duplicate,
        )?],
        None,
    )?;
    assert_eq!(storage.list_object_keys(), Err(StorageError::Unavailable));
    replay.assert_requests_match(&[]);

    let truncated = list_response(["valid"], true, None);
    let (_temporary, storage, replay) = replay_storage(
        vec![event(
            Method::GET,
            &list_uri(None, None),
            "",
            200,
            truncated,
        )?],
        None,
    )?;
    assert_eq!(storage.list_object_keys(), Err(StorageError::Unavailable));
    replay.assert_requests_match(&[]);
    Ok(())
}

#[test]
fn inventory_rejects_provider_keys_outside_prefix_and_unsafe_keys() -> TestResult {
    for (prefix, provider_key, expected_uri) in [
        (
            Some("tenant/core"),
            "other/key",
            list_uri(Some("tenant/core/"), None),
        ),
        (None, "../escape", list_uri(None, None)),
    ] {
        let response = list_response([provider_key], false, None);
        let (_temporary, storage, replay) = replay_storage(
            vec![event(Method::GET, &expected_uri, "", 200, response)?],
            prefix,
        )?;
        assert_eq!(storage.list_object_keys(), Err(StorageError::InvalidInput));
        replay.assert_requests_match(&[]);
    }
    Ok(())
}

#[test]
fn inventory_maps_provider_failures_and_empty_results() -> TestResult {
    let (_temporary, storage, replay) = replay_storage(
        vec![event(
            Method::GET,
            &list_uri(None, None),
            "",
            503,
            "<Error><Code>SlowDown</Code></Error>",
        )?],
        None,
    )?;
    assert_eq!(storage.list_object_keys(), Err(StorageError::Unavailable));
    replay.assert_requests_match(&[]);

    let (_temporary, storage, replay) = replay_storage(
        vec![event(
            Method::GET,
            &list_uri(None, None),
            "",
            200,
            list_response([], false, None),
        )?],
        None,
    )?;
    assert_eq!(storage.list_object_keys()?, Vec::<StorageKey>::new());
    replay.assert_requests_match(&[]);
    Ok(())
}

#[test]
fn repeated_continuation_token_fails_closed() -> TestResult {
    let first = list_response(["first"], true, Some("same"));
    let second = list_response(["second"], true, Some("same"));
    let events = vec![
        event(Method::GET, &list_uri(None, None), "", 200, first)?,
        event(Method::GET, &list_uri(None, Some("same")), "", 200, second)?,
    ];
    let (_temporary, storage, replay) = replay_storage(events, None)?;
    assert_eq!(storage.list_object_keys(), Err(StorageError::Unavailable));
    replay.assert_requests_match(&[]);
    Ok(())
}

fn list_uri(prefix: Option<&str>, continuation: Option<&str>) -> String {
    let mut query = vec![("list-type", "2")];
    if let Some(value) = prefix {
        query.push(("prefix", value));
    }
    if let Some(value) = continuation {
        query.push(("continuation-token", value));
    }
    query.sort_by_key(|(name, _value)| *name);
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(query)
        .finish();
    object_uri("", Some(&query))
}

fn list_response<const N: usize>(
    keys: [&str; N],
    truncated: bool,
    continuation: Option<&str>,
) -> String {
    let mut contents = String::new();
    for key in keys {
        contents.push_str("<Contents><Key>");
        contents.push_str(key);
        contents.push_str("</Key></Contents>");
    }
    let token = continuation.map_or_else(String::new, |value| {
        format!("<NextContinuationToken>{value}</NextContinuationToken>")
    });
    format!(
        "<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>{BUCKET}</Name><IsTruncated>{truncated}</IsTruncated>{contents}{token}</ListBucketResult>"
    )
}

#[test]
fn inventory_rejects_missing_object_keys() -> TestResult {
    let response = format!(
        "<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>{BUCKET}</Name><IsTruncated>false</IsTruncated><Contents /></ListBucketResult>"
    );
    let (_temporary, storage, replay) = replay_storage(
        vec![event(
            Method::GET,
            &list_uri(None, None),
            "",
            200,
            response,
        )?],
        None,
    )?;
    assert_eq!(storage.list_object_keys(), Err(StorageError::Unavailable));
    replay.assert_requests_match(&[]);
    Ok(())
}
