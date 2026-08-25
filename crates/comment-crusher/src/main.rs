// Concern: parses argv and runs the CLI, rendering a parse failure in the format the caller asked for | Non-concern: any logic (cli.rs owns it) | IO: (argv) -> process exit

use clap::Parser;
use clap::error::ErrorKind;
use comment_crusher::cli::{Cli, EXIT_BAD_ARGS, say};

fn main() -> std::process::ExitCode {
    if Cli::version_request() {
        return u8::try_from(Cli::version_only()).unwrap_or(1).into();
    }
    let code = match Cli::try_parse() {
        Ok(cli) => cli.run(),
        // Help and version are successful requests, not invalid usage.
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            say(&e.to_string());
            0
        }
        Err(e) if Cli::wants_json() => {
            let message = e.to_string();
            let first = message.lines().next().unwrap_or_default();
            say(&comment_crusher::cli::error_json("bad_arguments", first));
            EXIT_BAD_ARGS
        }
        // stdout, like every other channel a caller reads.
        Err(e) => {
            say(&e.to_string());
            EXIT_BAD_ARGS
        }
    };
    u8::try_from(code).unwrap_or(1).into()
}
