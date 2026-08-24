// Concern: parses argv and runs the CLI, rendering a parse failure in the format the caller asked for | Non-concern: any logic (cli.rs owns it) | IO: (argv) -> process exit

use clap::Parser;
use clap::error::ErrorKind;
use comment_crusher::cli::{Cli, EXIT_BAD_ARGS};

/// Off argv: clap has not run, and a rejection still owes an answer in the requested shape.
fn wants_json() -> bool {
    let mut args = std::env::args_os().map(|a| a.to_string_lossy().into_owned());
    while let Some(a) = args.next() {
        if a == "--format" && args.next().as_deref() == Some("json") {
            return true;
        }
        if a == "--format=json" {
            return true;
        }
    }
    false
}

fn version_request() -> bool {
    std::env::args_os().any(|a| a == "--version" || a == "-V")
}

fn main() -> std::process::ExitCode {
    if version_request() {
        return u8::try_from(Cli::version_only(wants_json()))
            .unwrap_or(1)
            .into();
    }
    let code = match Cli::try_parse() {
        Ok(cli) => cli.run(),
        // Help and version are successful requests, not invalid usage.
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            let _ = e.print();
            0
        }
        Err(e) if wants_json() => {
            let message = e.to_string();
            let first = message.lines().next().unwrap_or_default();
            println!(
                "{}",
                comment_crusher::cli::error_json("bad_arguments", first)
            );
            EXIT_BAD_ARGS
        }
        Err(e) => {
            let _ = e.print();
            EXIT_BAD_ARGS
        }
    };
    u8::try_from(code).unwrap_or(1).into()
}
