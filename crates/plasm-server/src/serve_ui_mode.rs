//! Whether the appliance runs the Ratatui control station or headless stderr logging.

use std::io::{stdin, stdout, IsTerminal};

use crate::ServeCli;

/// Resolve control-station vs headless serve mode.
///
/// Precedence: `--no-tui` → off; `--tui` → on; else on only when **both** stdout and stdin are TTYs.
pub(crate) fn serve_use_tui(cli: &ServeCli) -> bool {
    resolve_serve_use_tui(
        cli.no_tui,
        cli.tui,
        stdout().is_terminal(),
        stdin().is_terminal(),
    )
}

fn resolve_serve_use_tui(no_tui: bool, force_tui: bool, stdout_tty: bool, stdin_tty: bool) -> bool {
    if no_tui {
        return false;
    }
    if force_tui {
        return true;
    }
    stdout_tty && stdin_tty
}

#[cfg(test)]
mod tests {
    use super::resolve_serve_use_tui;

    #[test]
    fn explicit_no_tui_wins() {
        assert!(!resolve_serve_use_tui(true, true, true, true));
    }

    #[test]
    fn force_tui_wins_over_non_tty() {
        assert!(resolve_serve_use_tui(false, true, false, false));
    }

    #[test]
    fn auto_requires_both_ttys() {
        assert!(resolve_serve_use_tui(false, false, true, true));
        assert!(!resolve_serve_use_tui(false, false, true, false));
        assert!(!resolve_serve_use_tui(false, false, false, true));
        assert!(!resolve_serve_use_tui(false, false, false, false));
    }
}
