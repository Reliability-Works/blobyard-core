use super::ToolKind;

pub(super) const fn contract(kind: ToolKind) -> Option<(&'static str, Vec<&'static str>)> {
    match kind {
        ToolKind::Whoami => Some((
            "Show the authenticated Blobyard identity and selected scope.",
            vec![],
        )),
        ToolKind::ListWorkspaces => {
            Some(("List workspaces visible to the current identity.", vec![]))
        }
        ToolKind::ListProjects => {
            Some(("List projects visible in the selected workspace.", vec![]))
        }
        ToolKind::GetRetention => Some(("Show the selected project's retention policy.", vec![])),
        ToolKind::ListInboxes => Some(("List redacted inboxes in the selected project.", vec![])),
        ToolKind::ListShares => Some(("List redacted shares in the selected workspace.", vec![])),
        ToolKind::ListPreviews => Some(("List redacted previews in the selected project.", vec![])),
        ToolKind::ListWebYards => Some(("List Web Yards in the selected project.", vec![])),
        _ => None,
    }
}
