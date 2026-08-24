// Concern: parses argv, runs the CLI, and maps a failure to a nonzero exit | Non-concern: any logic | IO: (argv) -> process exit

use clap::Parser;
use comment_crusher::cli::Cli;

fn main() -> std::process::ExitCode {
    match Cli::parse().run() {
        Ok(code) => u8::try_from(code).unwrap_or(1).into(),
        Err(e) => {
            eprintln!("comment-crusher: {e:#}");
            2.into()
        }
    }
}
