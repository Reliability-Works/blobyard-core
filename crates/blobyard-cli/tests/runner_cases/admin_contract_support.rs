use blobyard_cli::{Diagnostics, GlobalArgs, OutputOptions, OutputRenderer, RenderedOutput};

pub(in crate::runner_cases_tests) fn human_stdout(result: blobyard_cli::CommandResult) -> String {
    human_output(result).stdout
}

pub(in crate::runner_cases_tests) fn human_output(
    result: blobyard_cli::CommandResult,
) -> RenderedOutput {
    OutputRenderer::new(
        OutputOptions::from_flags(&GlobalArgs {
            json: false,
            quiet: false,
            verbose: false,
            api_url: None,
            web_yard_origin: None,
            profile: None,
            workspace: None,
            project: None,
            retry_key: None,
        }),
        Diagnostics::default(),
    )
    .success(result)
}
