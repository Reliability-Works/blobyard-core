mod discovery;
mod download;
mod file_facts;
mod http;
mod identifiers;
pub(super) mod progress;
mod provenance;
mod resume;
mod upload;
mod upload_api;
mod upload_math;

#[cfg(test)]
mod http_sink_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod resume_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod upload_failure_tests;
