use super::*;

impl Fixture {
    /// Creates a complete fixture.
    ///
    /// # Panics
    ///
    /// Panics when command or fixture configuration is invalid.
    #[must_use]
    pub(in crate::runner_cases_tests) fn new(
        args: &[&str],
        responses: Vec<RawResponse>,
        environment_token: Option<&str>,
        store_token: Option<&str>,
    ) -> Self {
        Self::new_with_project_config(args, responses, environment_token, store_token, None)
    }

    /// Creates a complete fixture with a nearest project configuration file.
    #[must_use]
    pub(in crate::runner_cases_tests) fn with_project_config(
        args: &[&str],
        responses: Vec<RawResponse>,
        environment_token: Option<&str>,
        store_token: Option<&str>,
        project_config: &str,
    ) -> Self {
        Self::new_with_project_config(
            args,
            responses,
            environment_token,
            store_token,
            Some(project_config),
        )
    }

    fn new_with_project_config(
        args: &[&str],
        responses: Vec<RawResponse>,
        environment_token: Option<&str>,
        store_token: Option<&str>,
        project_config: Option<&str>,
    ) -> Self {
        let cli = Cli::try_parse_from(args).expect("command grammar");
        let temp = tempfile::tempdir().expect("tempdir");
        if let Some(project_config) = project_config {
            std::fs::write(temp.path().join(".blobyard.toml"), project_config)
                .expect("project config");
        }
        let mut values = HashMap::new();
        if let Some(token) = environment_token {
            values.insert("BLOBYARD_TOKEN".into(), token.into());
        }
        let environment = TestEnvironment(values);
        let config = ConfigLoader::new(
            ConfigPaths::new(temp.path(), temp.path().join("user/config.toml")),
            &environment,
        )
        .load(&cli.global)
        .expect("resolved config");
        let store = Arc::new(store_token.map_or_else(FakeStore::default, FakeStore::with_token));
        let transport = Arc::new(QueueTransport::new(responses));
        let runner = Runner::new(ApiClient::new(transport.clone()), config, store.clone())
            .with_retry_key(cli.global.retry_key.clone());
        Self {
            command: cli.command,
            runner,
            transport,
            store,
            temp,
        }
    }
}
