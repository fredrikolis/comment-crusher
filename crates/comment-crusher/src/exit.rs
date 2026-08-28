// Concern: how a run leaves — the exit codes an agent branches on, and the one channel every caller reads | Non-concern: what any code means to a rule (cli.rs prints the legend) | IO: (line) -> stdout

pub const EXIT_BAD_ARGS: i32 = 2;
pub const EXIT_VALIDATION: i32 = 3;
pub const EXIT_NOT_FOUND: i32 = 24;
pub const EXIT_INTERNAL: i32 = 1;

/// A closed pipe must not panic a linter in one.
pub fn say(line: &str) {
    use std::io::Write as _;
    let out = std::io::stdout();
    let mut out = out.lock();
    let wrote = writeln!(out, "{line}").and_then(|()| out.flush());
    // A closed reader got what it wanted; a full disk did not.
    if let Err(e) = wrote
        && e.kind() != std::io::ErrorKind::BrokenPipe
    {
        WRITE_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

static WRITE_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[must_use]
pub fn write_failed() -> bool {
    WRITE_FAILED.load(std::sync::atomic::Ordering::Relaxed)
}
