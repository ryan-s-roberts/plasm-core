# Appliance TUI — PTY integration coverage map

This inventory maps each control-station surface to the **PTY integration** test that exercises it. All coverage is **real process + real Crossterm + real embedded Postgres / auth KV** (no `TestBackend`, no stub admin bridge).

| Surface | Source module | PTY test file | Test name |
|--------|----------------|---------------|-----------|
| BOOT checklist / handoff to RUN | `boot.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` (implicit: boot completes before RUN title shows `q quit`) |
| RUN shell banner, HTTP/MCP ports | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` |
| Tab navigation (Status → Clients → APIs → OAuth → Keys) | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` |
| OAuth tab — upsert wizard open (`n`) + Esc cancel | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_oauth_wizard_esc_cancel` |
| Status tab — listeners / policy line | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` |
| Clients tab — MCP URL | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` |
| Keys tab — add key (`a`), label, provision result | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` |
| Footer help `?` / dismiss | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` |
| Clean quit `q` | `tui.rs` | `tui_pty_integration.rs` | `tui_pty_full_suite` |

## Not covered in default PTY suite

| Surface | Reason |
|--------|--------|
| APIs tab filter / `Space` toggle | Keystroke-heavy; extend `tui_pty_integration.rs` when stable snapshot strings are locked. |
| OAuth device bind (`d`) | Long / interactive; manual or `#[ignore]` extended scenario only. |
| Logs tab `PgDn` / `g` | Optional follow-up once log buffer strings are stable under `NO_COLOR=1`. |
| Async footer (`Refreshing…`, `Provisioning key…`) | PTY suite waits on final provision line; RUN mode no longer polls full DB refresh on a timer (only startup + post-mutation). |

## How to run

See [docs/appliance-surface-inventory.md](../../../../docs/appliance-surface-inventory.md) (includes **OAuth CLI vs TUI parity** table) — prefer `bash scripts/appliance-tui-pty-tests.sh` (watchdog + serialized threads); or `RUST_TEST_THREADS=1 PLASM_TUI_PTY_TESTS=1 cargo test -p plasm-server --features tui_pty_tests --test tui_pty_integration -- --test-threads=1`.
