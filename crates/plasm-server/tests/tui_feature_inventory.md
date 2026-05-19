# plasm-server TUI test inventory

| Surface | Code | Unit (`tui/mod.rs` tests) | Integration |
|--------|------|---------------------------|-------------|
| BOOT checklist / RUN handoff | `boot.rs`, `main.rs` | — | `appliance_headless_boot` (diag); PTY smoke waits for RUN footer |
| RUN shell / quit `q` | `tui/mod.rs` | `run_footer_includes_quit_hint` | `plasm-server-pty-tests` `tui_pty_quit_smoke` |
| Tab navigation | `tui/mod.rs` | `run_screen_wraps_left_and_right` | PTY smoke (implicit: footer visible) |
| Status — listeners | `tui/mod.rs` | — | headless diag |
| Clients — Bearer / curl | `tui/mod.rs` | `clients_curl_snippet_uses_bearer_header` | — |
| APIs — filter bar | `tui/mod.rs` | `apis_filter_bar_heading`, `api_filter_mode_enters_and_esc_clears` | — |
| OAuth — wizard Esc cancel | `tui/mod.rs` | `oauth_wizard_esc_sets_cancel_notice`, `oauth_disable_confirm_cancels_cleanly` | — |
| Keys — add / footer | `tui/mod.rs` | `keys_tab_footer_includes_add`, `add_key_modal_confirms_and_cancels` | — |
| Help overlay | `tui/mod.rs` | `help_overlay_documents_keys_tab_shortcuts` | — |

CI: `bash scripts/appliance-tui-pty-tests.sh` (headless gate + one PTY quit smoke).
