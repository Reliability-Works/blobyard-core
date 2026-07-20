use url::form_urlencoded::Serializer;

use blobyard_core::Slug;

pub(super) fn query(pairs: &[(&str, Option<String>)]) -> String {
    let mut serializer = Serializer::new(String::new());
    for (name, value) in pairs {
        if let Some(value) = value {
            serializer.append_pair(name, value);
        }
    }
    serializer.finish()
}

pub(super) fn scoped_query(
    workspace: &Slug,
    project: &Slug,
    mut suffix: Vec<(&'static str, Option<String>)>,
) -> String {
    let mut pairs = vec![
        ("workspace", Some(workspace.to_string())),
        ("project", Some(project.to_string())),
    ];
    pairs.append(&mut suffix);
    query(&pairs)
}
