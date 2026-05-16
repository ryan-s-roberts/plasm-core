//! Plain stderr lines for operators (scrollback, systemd, pipes). Separate from the alternate-screen TUI.

use std::io::Write;

pub(crate) fn line(msg: impl AsRef<str>) {
    let m = msg.as_ref();
    eprintln!("{m}");
    let _ = std::io::stderr().flush();
}
