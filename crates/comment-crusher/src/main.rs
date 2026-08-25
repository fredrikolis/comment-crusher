// Concern: parses argv and runs the CLI, rendering a parse failure in the format the caller asked for | Non-concern: any logic (cli.rs owns it) | IO: (argv) -> process exit

use clap::Parser;
use clap::error::ErrorKind;
use comment_crusher::cli::{Cli, EXIT_BAD_ARGS};

fn main() -> std::process::ExitCode {
    if Cli::version_request() {
        return u8::try_from(Cli::version_only(Cli::wants_json()))
            .unwrap_or(1)
            .into();
    }
    let code = match Cli::try_parse() {
        Ok(cli) => cli.run(),
        // Help and version are successful requests, not invalid usage.
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            println!("{e}");
            0
        }
        Err(e) if Cli::wants_json() => {
            let message = e.to_string();
            let first = message.lines().next().unwrap_or_default();
            println!(
                "{}",
                comment_crusher::cli::error_json("bad_arguments", first)
            );
            EXIT_BAD_ARGS
        }
        // stdout, like every other channel a caller reads.
        Err(e) => {
            println!("{e}");
            EXIT_BAD_ARGS
        }
    };
    u8::try_from(code).unwrap_or(1).into()
}
