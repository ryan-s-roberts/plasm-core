//! Plain stderr lines for operators (scrollback, systemd, pipes). Separate from the alternate-screen TUI.

use std::cell::Cell;
use std::io::Write;

thread_local! {
    static ALTERNATE_SCREEN_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// Set while Ratatui owns the alternate screen — [`line`] routes to tracing instead of stderr.
pub(crate) fn set_alternate_screen_active(active: bool) {
    ALTERNATE_SCREEN_ACTIVE.with(|c| c.set(active));
}

pub(crate) fn line(msg: impl AsRef<str>) {
    let m = msg.as_ref();
    if ALTERNATE_SCREEN_ACTIVE.with(|c| c.get()) {
        tracing::info!(target: "plasm_appliance", "{m}");
    } else {
        eprintln!("{m}");
        let _ = std::io::stderr().flush();
    }
}
