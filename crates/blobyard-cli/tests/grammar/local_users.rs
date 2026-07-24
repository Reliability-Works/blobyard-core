pub(crate) const LOCAL_USER_COMMANDS: &[&[&str]] = &[
    &["blobyard", "users", "list"],
    &[
        "blobyard",
        "--workspace",
        "team",
        "users",
        "create",
        "Ada Lovelace",
        "--email",
        "ada@example.test",
    ],
    &["blobyard", "users", "reset-key", "user_1"],
    &["blobyard", "users", "deactivate", "user_1"],
];
