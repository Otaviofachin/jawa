use crate::system::runner::{CommandOutput, FakeRunner};

pub(crate) const SYSTEM_DF: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/docker/system-df.jsonl"
));
pub(crate) const SYSTEM_DF_VERBOSE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/docker/system-df-v.json"
));
pub(crate) const DANGLING: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/docker/volume-ls-dangling.jsonl"
));
pub(crate) const STOPPED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/docker/stopped.txt"
));

pub(crate) fn ok(stdout: &str) -> CommandOutput {
    CommandOutput {
        success: true,
        code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

pub(crate) fn running_docker() -> FakeRunner {
    FakeRunner::new()
        .on(&["system", "df", "--format"], ok(SYSTEM_DF))
        .on(&["system", "df", "-v"], ok(SYSTEM_DF_VERBOSE))
        .on(&["volume", "ls"], ok(DANGLING))
        .on(&["ps", "-a"], ok(STOPPED))
        .on(&["context", "show"], ok("desktop-linux\n"))
}
