//! Ratatui control station — Your MCP–oriented tabs over [`PlasmHostState`] (no loopback HTTP).

use std::collections::{HashSet, VecDeque};
use std::io::{self, stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use plasm_agent_core::mcp_config_admin::{McpConfigApiKeyRow, McpConfigCatalogRow};
use plasm_agent_core::server_state::PlasmHostState;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;
use uuid::Uuid;

use crate::appliance_admin_bridge::{
    AdminBridge, AdminCompletion, AdminCorr, AdminJob, McpConfigSurfaceState, OAuthSurfaceState,
    RefreshedUiData,
};
use crate::appliance_log;
use crate::appliance_mcp_admin::appliance_mcp_scope;
use crate::boot::UiEvent;
use crate::oauth_upsert_wizard::{OAuthUpsertStep, OAuthUpsertWizard};

/// Raw TTY (`cfmakeraw`) does not raise SIGINT on ^C — the byte is delivered as input. Match that
/// here so Ctrl+C still exits the control station (Tokio `ctrl_c()` alone never fires in raw mode).
///
/// **tui-design note:** the TUI skill discourages binding terminal-owned chords (`Ctrl+C`, etc.).
/// This path is an intentional exception: without it, users cannot interrupt the alternate-screen
/// loop from the keyboard because no SIGINT reaches the Tokio handler. Primary quit remains `q`.
fn raw_tty_wants_process_quit(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('\x03'))
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C')))
}

fn mcp_streamable_url(mcp_port: u16) -> String {
    format!("http://127.0.0.1:{mcp_port}/mcp")
}

fn mcp_curl_snippet(mcp_port: u16) -> String {
    let url = mcp_streamable_url(mcp_port);
    format!(r#"curl -sS -H "Authorization: Bearer $PLASM_MCP_KEY" {url}"#)
}

fn api_key_row_label(k: &McpConfigApiKeyRow) -> String {
    match k.label.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => format!("(unnamed · {})", uuid_head(&k.key_id)),
    }
}

fn uuid_head(id: &Uuid) -> String {
    id.hyphenated()
        .to_string()
        .split('-')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn api_key_row_copy_line(k: &McpConfigApiKeyRow) -> String {
    format!("{}  {}", api_key_row_label(k), k.key_id)
}

fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text).map_err(|e| e.to_string())
}

fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

fn run_title_style() -> Style {
    let mut s = Style::default().add_modifier(Modifier::BOLD);
    if !no_color() {
        s = s.fg(Color::Cyan);
    }
    s
}

fn run_badge_style() -> Style {
    let s = Style::default().add_modifier(Modifier::BOLD);
    if no_color() {
        return s;
    }
    s.fg(Color::Black).bg(Color::Green)
}

fn dim_style() -> Style {
    let mut s = Style::default();
    if !no_color() {
        s = s.fg(Color::DarkGray);
    } else {
        s = s.add_modifier(Modifier::DIM);
    }
    s
}

fn err_emphasis_style() -> Style {
    let mut s = Style::default().add_modifier(Modifier::BOLD);
    if !no_color() {
        s = s.fg(Color::Red);
    }
    s
}

fn api_toggle_on_style() -> Style {
    let mut s = Style::default().add_modifier(Modifier::BOLD);
    if !no_color() {
        s = s.fg(Color::Green);
    }
    s
}

fn api_toggle_off_style() -> Style {
    let mut s = Style::default();
    if !no_color() {
        s = s.fg(Color::DarkGray);
    } else {
        s = s.add_modifier(Modifier::DIM);
    }
    s
}

fn catalog_row_display_name(entry_id: &str, label: &str) -> String {
    if label.trim() == entry_id.trim() {
        entry_id.to_string()
    } else {
        format!("{entry_id} — {label}")
    }
}

#[derive(Default)]
struct UiSnapshot {
    config_surface: McpConfigSurfaceState,
    catalog_rows: Vec<McpConfigCatalogRow>,
    keys: Vec<McpConfigApiKeyRow>,
    db_allowed: HashSet<String>,
    oauth_providers: Vec<plasm_agent_core::oauth_provider_repository::OauthProviderAppRow>,
    oauth_binding_hints: Vec<String>,
    oauth_surface: OAuthSurfaceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunScreen {
    Status,
    Clients,
    Apis,
    OAuth,
    Keys,
    Runs,
    Storage,
    Logs,
}

impl RunScreen {
    const ALL: [Self; 8] = [
        Self::Status,
        Self::Clients,
        Self::Apis,
        Self::OAuth,
        Self::Keys,
        Self::Runs,
        Self::Storage,
        Self::Logs,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Clients => "Clients",
            Self::Apis => "APIs",
            Self::OAuth => "OAuth",
            Self::Keys => "Keys",
            Self::Runs => "Runs",
            Self::Storage => "Storage",
            Self::Logs => "Logs",
        }
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
        Self::ALL[self.index().checked_sub(1).unwrap_or(Self::ALL.len() - 1)]
    }

    fn index(self) -> usize {
        match self {
            Self::Status => 0,
            Self::Clients => 1,
            Self::Apis => 2,
            Self::OAuth => 3,
            Self::Keys => 4,
            Self::Runs => 5,
            Self::Storage => 6,
            Self::Logs => 7,
        }
    }
}

#[derive(Clone, Debug)]
enum InputMode {
    Normal,
    ApiFilter,
    AddKeyLabel { buf: String },
    OAuthWizard(OAuthUpsertWizard),
    ConfirmOAuthDisable { entry_id: String },
    ConfirmKeyRevoke { key_id: Uuid },
}

#[derive(Default)]
struct ApiState {
    selected: usize,
    /// Indices into `snapshot.catalog_rows` after filter.
    filtered_ix: Vec<usize>,
    filter: String,
    staged_allowed: Option<HashSet<String>>,
}

#[derive(Default)]
struct OAuthState {
    selected: usize,
}

#[derive(Default)]
struct KeysState {
    selected: usize,
}

#[derive(Default)]
struct LogState {
    lines: VecDeque<String>,
    scroll: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdminTaskKind {
    Refreshing,
    ProvisioningKey,
    SavingApiAllowlist,
    DeviceAuthorization,
    SavingOAuthProvider,
    DisablingOAuthProvider,
    RotatingKey,
    RevokingKey,
    RevealingKey,
}

impl AdminTaskKind {
    fn label(self) -> &'static str {
        match self {
            Self::Refreshing => "Refreshing…",
            Self::ProvisioningKey => "Provisioning key…",
            Self::SavingApiAllowlist => "Saving API allowlist…",
            Self::DeviceAuthorization => "Device authorization…",
            Self::SavingOAuthProvider => "Saving OAuth provider…",
            Self::DisablingOAuthProvider => "Disabling OAuth provider…",
            Self::RotatingKey => "Rotating key…",
            Self::RevokingKey => "Revoking key…",
            Self::RevealingKey => "Revealing key…",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PendingAdminTask {
    corr: AdminCorr,
    kind: AdminTaskKind,
    started_at: Instant,
}

#[derive(Default)]
struct AdminSyncState {
    /// Monotonic correlation id for async admin jobs.
    next_corr: AdminCorr,
    refresh: Option<PendingAdminTask>,
    inline: Option<PendingAdminTask>,
}

impl AdminSyncState {
    fn pending_refresh_corr(&self) -> Option<AdminCorr> {
        self.refresh.map(|task| task.corr)
    }

    fn pending_inline_corr(&self) -> Option<AdminCorr> {
        self.inline.map(|task| task.corr)
    }

    fn start_refresh(&mut self, corr: AdminCorr) {
        self.refresh = Some(PendingAdminTask {
            corr,
            kind: AdminTaskKind::Refreshing,
            started_at: Instant::now(),
        });
    }

    fn start_inline(&mut self, corr: AdminCorr, kind: AdminTaskKind) {
        self.inline = Some(PendingAdminTask {
            corr,
            kind,
            started_at: Instant::now(),
        });
    }

    fn finish_refresh(&mut self, corr: AdminCorr) -> bool {
        if self.pending_refresh_corr() == Some(corr) {
            self.refresh = None;
            return true;
        }
        false
    }

    fn finish_inline(&mut self, corr: AdminCorr) -> Option<AdminTaskKind> {
        if self.pending_inline_corr() == Some(corr) {
            return self.inline.take().map(|task| task.kind);
        }
        None
    }

    fn busy_task(&self) -> Option<PendingAdminTask> {
        self.inline.or(self.refresh)
    }
}

#[derive(Default)]
struct ResourceState {
    snapshot: UiSnapshot,
    config_id: Option<Uuid>,
    admin: AdminSyncState,
}

struct RunState {
    screen: RunScreen,
    mode: InputMode,
    api: ApiState,
    oauth: OAuthState,
    keys: KeysState,
    logs: LogState,
    resources: ResourceState,
    status_msg: String,
    show_help: bool,
}

impl RunState {
    fn new() -> Self {
        Self {
            screen: RunScreen::Status,
            mode: InputMode::Normal,
            api: ApiState::default(),
            oauth: OAuthState::default(),
            keys: KeysState::default(),
            logs: LogState::default(),
            resources: ResourceState::default(),
            status_msg: String::new(),
            show_help: false,
        }
    }

    fn recompute_filter(&mut self, rows: &[McpConfigCatalogRow]) {
        let f = self.api.filter.trim().to_ascii_lowercase();
        self.api.filtered_ix.clear();
        for (i, r) in rows.iter().enumerate() {
            if f.is_empty()
                || r.entry_id.to_ascii_lowercase().contains(&f)
                || r.label.to_ascii_lowercase().contains(&f)
            {
                self.api.filtered_ix.push(i);
            }
        }
        if self.api.selected >= self.api.filtered_ix.len() {
            self.api.selected = self.api.filtered_ix.len().saturating_sub(1);
        }
    }

    fn add_key_label_buf(&self) -> Option<&str> {
        match &self.mode {
            InputMode::AddKeyLabel { buf } => Some(buf.as_str()),
            _ => None,
        }
    }

    fn pending_oauth_disable_entry(&self) -> Option<&str> {
        match &self.mode {
            InputMode::ConfirmOAuthDisable { entry_id } => Some(entry_id.as_str()),
            _ => None,
        }
    }

    fn admin_busy(&self) -> bool {
        self.resources.admin.inline.is_some()
    }

    fn reset_screen_local_mode(&mut self) {
        let reset = matches!(
            (&self.screen, &self.mode),
            (RunScreen::Apis, InputMode::ApiFilter)
                | (RunScreen::OAuth, InputMode::OAuthWizard(_))
                | (RunScreen::OAuth, InputMode::ConfirmOAuthDisable { .. })
                | (RunScreen::Keys, InputMode::AddKeyLabel { .. })
                | (RunScreen::Keys, InputMode::ConfirmKeyRevoke { .. })
        );
        if !reset && !matches!(self.mode, InputMode::Normal) {
            self.mode = InputMode::Normal;
        }
    }
}

fn alloc_admin_corr(state: &mut RunState) -> AdminCorr {
    state.resources.admin.next_corr = state.resources.admin.next_corr.wrapping_add(1).max(1);
    state.resources.admin.next_corr
}

fn enqueue_refresh_if_idle(state: &mut RunState, bridge: &AdminBridge) {
    if state.resources.admin.refresh.is_some() {
        return;
    }
    enqueue_refresh_force(state, bridge);
}

/// Queue a full snapshot refresh and supersede any in-flight refresh correlation (stale completions ignored).
fn enqueue_refresh_force(state: &mut RunState, bridge: &AdminBridge) {
    let c = alloc_admin_corr(state);
    state.resources.admin.start_refresh(c);
    if bridge
        .jobs_tx
        .send(AdminJob::RefreshFull { corr: c })
        .is_err()
    {
        state.resources.admin.refresh = None;
        state.status_msg = "Admin router queue closed — restart the appliance.".into();
    }
}

fn submit_inline_admin_job(
    state: &mut RunState,
    bridge: &AdminBridge,
    kind: AdminTaskKind,
    build: impl FnOnce(AdminCorr) -> AdminJob,
) {
    let c = alloc_admin_corr(state);
    state.resources.admin.start_inline(c, kind);
    let job = build(c);
    if bridge.jobs_tx.send(job).is_err() {
        state.resources.admin.inline = None;
        state.status_msg = "Admin router queue closed — restart the appliance.".into();
    }
}

fn apply_refreshed_ui_data(state: &mut RunState, data: RefreshedUiData) {
    state.resources.config_id = data.config_id;
    state.resources.snapshot.config_surface = data.config_surface;
    state.resources.snapshot.catalog_rows = data.catalog_rows;
    state.resources.snapshot.keys = data.keys;
    state.resources.snapshot.db_allowed = data.db_allowed;
    state.resources.snapshot.oauth_providers = data.oauth_providers;
    state.resources.snapshot.oauth_binding_hints = data.oauth_binding_hints;
    state.resources.snapshot.oauth_surface = data.oauth_surface;
}

fn apply_admin_completion(
    state: &mut RunState,
    bridge: Option<&AdminBridge>,
    comp: AdminCompletion,
) {
    match comp {
        AdminCompletion::RefreshFull { corr, data } => {
            if state.resources.admin.finish_refresh(corr) {
                apply_refreshed_ui_data(state, data);
                let rows = state.resources.snapshot.catalog_rows.clone();
                state.recompute_filter(&rows);
                if state.oauth.selected >= state.resources.snapshot.oauth_providers.len() {
                    state.oauth.selected = state
                        .resources
                        .snapshot
                        .oauth_providers
                        .len()
                        .saturating_sub(1);
                }
            }
        }
        AdminCompletion::ProvisionApiKey { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(p) => {
                        state.status_msg = format!("provisioned key_id={}", p.key_id);
                    }
                    Err(e) => state.status_msg = format!("provision error: {e}"),
                }
                if let Some(bridge) = bridge {
                    enqueue_refresh_force(state, bridge);
                }
            }
        }
        AdminCompletion::SetAllowedApisExact { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(()) => {
                        state.status_msg = "saved API allowlist".into();
                        state.api.staged_allowed = None;
                    }
                    Err(e) => state.status_msg = format!("save error: {e}"),
                }
                if let Some(bridge) = bridge {
                    enqueue_refresh_force(state, bridge);
                }
            }
        }
        AdminCompletion::OAuthDeviceBind { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(out) => {
                        state.status_msg = format!(
                            "device ok · open {} · user_code {} · {}",
                            out.verification_uri, out.user_code, out.hosted_kv_key
                        );
                    }
                    Err(e) => state.status_msg = format!("device bind failed: {e}"),
                }
                if let Some(bridge) = bridge {
                    enqueue_refresh_force(state, bridge);
                }
            }
        }
        AdminCompletion::OauthProviderUpsert { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(()) => state.status_msg = "OAuth provider saved.".into(),
                    Err(e) => state.status_msg = format!("OAuth upsert failed: {e}"),
                }
                if let Some(bridge) = bridge {
                    enqueue_refresh_force(state, bridge);
                }
            }
        }
        AdminCompletion::OauthProviderDisable { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(()) => state.status_msg = "OAuth provider disabled.".into(),
                    Err(e) => state.status_msg = format!("OAuth disable failed: {e}"),
                }
                if let Some(bridge) = bridge {
                    enqueue_refresh_force(state, bridge);
                }
            }
        }
        AdminCompletion::RotateApiKey { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(p) => {
                        state.status_msg = format!("rotated new key_id={}", p.key_id);
                    }
                    Err(e) => state.status_msg = format!("rotate error: {e}"),
                }
                if let Some(bridge) = bridge {
                    enqueue_refresh_force(state, bridge);
                }
            }
        }
        AdminCompletion::RevokeApiKey { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(()) => state.status_msg = "revoked key".into(),
                    Err(e) => state.status_msg = format!("revoke error: {e}"),
                }
                if let Some(bridge) = bridge {
                    enqueue_refresh_force(state, bridge);
                }
            }
        }
        AdminCompletion::RevealApiKey { corr, result } => {
            if state.resources.admin.finish_inline(corr).is_some() {
                match result {
                    Ok(raw) => state.status_msg = format!("revealed key: {raw}"),
                    Err(e) => state.status_msg = format!("reveal error: {e}"),
                }
            }
        }
    }
}

enum UiMsg {
    Tick,
    Key(KeyEvent),
    Admin(AdminCompletion),
    LogLine(String),
}

fn row_enabled(state: &RunState, snap: &UiSnapshot, entry_id: &str) -> bool {
    if let Some(ref staged) = state.api.staged_allowed {
        return staged.contains(entry_id);
    }
    snap.db_allowed.contains(entry_id)
}

fn oauth_surface_status(snap: &UiSnapshot) -> Option<&str> {
    snap.oauth_surface.status_message()
}

struct UpdateDeps<'a> {
    admin_bridge: Option<&'a AdminBridge>,
    host_state: Option<&'a PlasmHostState>,
    mcp_port: u16,
}

fn update_modal_key(state: &mut RunState, key: KeyEvent, deps: &UpdateDeps<'_>) -> bool {
    match &mut state.mode {
        InputMode::ApiFilter => match key.code {
            KeyCode::Enter | KeyCode::Tab | KeyCode::BackTab => state.mode = InputMode::Normal,
            KeyCode::Esc => {
                state.mode = InputMode::Normal;
                state.api.filter.clear();
                let rows = state.resources.snapshot.catalog_rows.clone();
                state.recompute_filter(&rows);
            }
            KeyCode::Backspace => {
                state.api.filter.pop();
                let rows = state.resources.snapshot.catalog_rows.clone();
                state.recompute_filter(&rows);
            }
            KeyCode::Char(c) => {
                state.api.filter.push(c);
                let rows = state.resources.snapshot.catalog_rows.clone();
                state.recompute_filter(&rows);
            }
            _ => {}
        },
        InputMode::AddKeyLabel { buf } => match key.code {
            KeyCode::Enter => {
                let label = buf.trim().to_string();
                state.mode = InputMode::Normal;
                if !label.is_empty() {
                    if state.admin_busy() {
                        state.status_msg = "Busy — wait for the current admin task.".into();
                    } else if let (Some(bridge), Some(cid)) =
                        (deps.admin_bridge, state.resources.config_id)
                    {
                        submit_inline_admin_job(
                            state,
                            bridge,
                            AdminTaskKind::ProvisioningKey,
                            |c| AdminJob::ProvisionApiKey {
                                corr: c,
                                config_id: cid,
                                label,
                            },
                        );
                    } else if deps.admin_bridge.is_some() && state.resources.config_id.is_none() {
                        state.status_msg = "MCP config still loading — wait for refresh.".into();
                    }
                }
            }
            KeyCode::Esc => state.mode = InputMode::Normal,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        },
        InputMode::OAuthWizard(wiz) => {
            let rows = &state.resources.snapshot.catalog_rows;
            match key.code {
                KeyCode::Esc => {
                    state.mode = InputMode::Normal;
                    state.status_msg = "OAuth provider wizard cancelled.".into();
                }
                KeyCode::Enter => {
                    if wiz.step == OAuthUpsertStep::Confirm {
                        match wiz.try_build_upsert() {
                            Ok(upsert) => {
                                if state.admin_busy() {
                                    state.status_msg =
                                        "Busy — wait for the current admin task.".into();
                                } else if let Some(bridge) = deps.admin_bridge {
                                    state.mode = InputMode::Normal;
                                    submit_inline_admin_job(
                                        state,
                                        bridge,
                                        AdminTaskKind::SavingOAuthProvider,
                                        |c| AdminJob::OauthProviderUpsert { corr: c, upsert },
                                    );
                                } else {
                                    state.status_msg =
                                        "Admin bridge unavailable — cannot save.".into();
                                }
                            }
                            Err(e) => state.status_msg = format!("OAuth upsert: {e}"),
                        }
                    } else if wiz.step == OAuthUpsertStep::Enabled {
                        wiz.advance_enabled_to_confirm();
                    } else if wiz.step == OAuthUpsertStep::EntryId {
                        if let Err(msg) = wiz.commit_entry_selection(rows) {
                            state.status_msg = msg.to_string();
                        }
                    } else if let Err(msg) = wiz.commit_buf_and_advance() {
                        state.status_msg = msg.to_string();
                    }
                }
                KeyCode::Down | KeyCode::Char('j') if wiz.step == OAuthUpsertStep::EntryId => {
                    wiz.move_entry_selection(rows, 1);
                }
                KeyCode::Up | KeyCode::Char('k') if wiz.step == OAuthUpsertStep::EntryId => {
                    wiz.move_entry_selection(rows, -1);
                }
                KeyCode::Char(' ') if wiz.step == OAuthUpsertStep::Enabled => {
                    wiz.enabled = !wiz.enabled;
                }
                KeyCode::Backspace
                    if !matches!(
                        wiz.step,
                        OAuthUpsertStep::Enabled | OAuthUpsertStep::Confirm
                    ) =>
                {
                    wiz.buf.pop();
                    if wiz.step == OAuthUpsertStep::EntryId {
                        wiz.reset_entry_selection();
                    }
                }
                KeyCode::Char(c)
                    if !matches!(
                        wiz.step,
                        OAuthUpsertStep::Enabled | OAuthUpsertStep::Confirm
                    ) =>
                {
                    wiz.buf.push(c);
                    if wiz.step == OAuthUpsertStep::EntryId {
                        wiz.reset_entry_selection();
                    }
                }
                _ => {}
            }
        }
        InputMode::Normal
        | InputMode::ConfirmOAuthDisable { .. }
        | InputMode::ConfirmKeyRevoke { .. } => {}
    }
    false
}

fn update_normal_key(state: &mut RunState, key: KeyEvent, deps: &UpdateDeps<'_>) -> bool {
    let snap = &state.resources.snapshot;
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('?') => state.show_help = true,
        KeyCode::Char('#') if state.screen == RunScreen::Clients => {
            let url = mcp_streamable_url(deps.mcp_port);
            state.status_msg = match copy_text_to_clipboard(&url) {
                Ok(()) => "clipboard: MCP URL".into(),
                Err(e) => format!("clipboard: {e}"),
            };
        }
        KeyCode::Char('%') if state.screen == RunScreen::Clients => {
            let curl = mcp_curl_snippet(deps.mcp_port);
            state.status_msg = match copy_text_to_clipboard(&curl) {
                Ok(()) => "clipboard: curl snippet".into(),
                Err(e) => format!("clipboard: {e}"),
            };
        }
        KeyCode::Char('#') if state.screen == RunScreen::Keys => {
            if let Some(k) = snap.keys.get(state.keys.selected) {
                let line = api_key_row_copy_line(k);
                state.status_msg = match copy_text_to_clipboard(&line) {
                    Ok(()) => "clipboard: key row".into(),
                    Err(e) => format!("clipboard: {e}"),
                };
            } else {
                state.status_msg = "no key row to copy (list empty)".into();
            }
        }
        KeyCode::Right | KeyCode::Tab => {
            state.screen = state.screen.next();
            state.reset_screen_local_mode();
        }
        KeyCode::Left | KeyCode::BackTab => {
            state.screen = state.screen.prev();
            state.reset_screen_local_mode();
        }
        KeyCode::Esc
            if state.screen == RunScreen::OAuth
                && matches!(state.mode, InputMode::ConfirmOAuthDisable { .. }) =>
        {
            state.mode = InputMode::Normal;
            state.status_msg = "OAuth disable cancelled.".into();
        }
        KeyCode::Esc
            if state.screen == RunScreen::Keys
                && matches!(state.mode, InputMode::ConfirmKeyRevoke { .. }) =>
        {
            state.mode = InputMode::Normal;
        }
        KeyCode::Char('/') if state.screen == RunScreen::Apis => {
            state.mode = InputMode::ApiFilter;
        }
        KeyCode::Down | KeyCode::Char('j') if state.screen == RunScreen::Apis => {
            if state.api.selected + 1 < state.api.filtered_ix.len() {
                state.api.selected += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') if state.screen == RunScreen::Apis => {
            state.api.selected = state.api.selected.saturating_sub(1);
        }
        KeyCode::Char(' ') if state.screen == RunScreen::Apis => {
            if let Some(&rix) = state.api.filtered_ix.get(state.api.selected) {
                if let Some(r) = snap.catalog_rows.get(rix) {
                    let eid = r.entry_id.clone();
                    if state.api.staged_allowed.is_none() {
                        state.api.staged_allowed = Some(snap.db_allowed.clone());
                    }
                    if let Some(ref mut set) = state.api.staged_allowed {
                        if set.contains(&eid) {
                            set.remove(&eid);
                        } else {
                            set.insert(eid);
                        }
                    }
                }
            }
        }
        KeyCode::Char('s') if state.screen == RunScreen::Apis => {
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if let (Some(bridge), Some(cid)) = (deps.admin_bridge, state.resources.config_id)
            {
                let set = state
                    .api
                    .staged_allowed
                    .clone()
                    .unwrap_or_else(|| snap.db_allowed.clone());
                submit_inline_admin_job(state, bridge, AdminTaskKind::SavingApiAllowlist, |c| {
                    AdminJob::SetAllowedApisExact {
                        corr: c,
                        config_id: cid,
                        entry_ids: set,
                    }
                });
            }
        }
        KeyCode::Down | KeyCode::Char('j') if state.screen == RunScreen::OAuth => {
            if state.oauth.selected + 1 < snap.oauth_providers.len() {
                state.oauth.selected += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') if state.screen == RunScreen::OAuth => {
            state.oauth.selected = state.oauth.selected.saturating_sub(1);
        }
        KeyCode::Char('n') if state.screen == RunScreen::OAuth => {
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if !snap.oauth_surface.services_ready() {
                state.status_msg = oauth_surface_status(snap)
                    .unwrap_or("OAuth services unavailable")
                    .to_string();
            } else {
                state.mode = InputMode::OAuthWizard(OAuthUpsertWizard::new());
            }
        }
        KeyCode::Char('x') if state.screen == RunScreen::OAuth => {
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if let Some(row) = snap.oauth_providers.get(state.oauth.selected) {
                state.mode = InputMode::ConfirmOAuthDisable {
                    entry_id: row.entry_id.clone(),
                };
                state.status_msg = "Press y to confirm disable (Esc cancels)".into();
            } else {
                state.status_msg = "no provider selected to disable".into();
            }
        }
        KeyCode::Char('y')
            if state.screen == RunScreen::OAuth
                && matches!(state.mode, InputMode::ConfirmOAuthDisable { .. }) =>
        {
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if let Some(bridge) = deps.admin_bridge {
                let entry_id = match std::mem::replace(&mut state.mode, InputMode::Normal) {
                    InputMode::ConfirmOAuthDisable { entry_id } => entry_id,
                    _ => String::new(),
                };
                submit_inline_admin_job(
                    state,
                    bridge,
                    AdminTaskKind::DisablingOAuthProvider,
                    |c| AdminJob::OauthProviderDisable { corr: c, entry_id },
                );
            } else {
                state.mode = InputMode::Normal;
                state.status_msg =
                    "Admin bridge unavailable — cannot disable OAuth provider.".into();
            }
        }
        KeyCode::Char('d') if state.screen == RunScreen::OAuth => {
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if !snap.oauth_surface.services_ready() {
                state.status_msg = oauth_surface_status(snap)
                    .unwrap_or("OAuth services unavailable")
                    .to_string();
            } else if let (Some(bridge), Some(row)) = (
                deps.admin_bridge,
                snap.oauth_providers.get(state.oauth.selected),
            ) {
                let entry_id = row.entry_id.clone();
                let host_state = match deps.host_state {
                    Some(host_state) => host_state,
                    None => {
                        state.status_msg = "OAuth host state unavailable".into();
                        return false;
                    }
                };
                let catalog = match host_state.oauth_link_catalog() {
                    Some(c) => Arc::clone(c),
                    None => {
                        state.status_msg = "OAuth catalog unavailable".into();
                        return false;
                    }
                };
                let storage = match host_state.auth_storage() {
                    Some(s) => Arc::clone(s),
                    None => {
                        state.status_msg = "auth storage unavailable".into();
                        return false;
                    }
                };
                submit_inline_admin_job(state, bridge, AdminTaskKind::DeviceAuthorization, |c| {
                    AdminJob::OAuthDeviceBind {
                        corr: c,
                        entry_id,
                        scopes: vec![],
                        catalog,
                        storage,
                    }
                });
            }
        }
        KeyCode::Down | KeyCode::Char('j') if state.screen == RunScreen::Keys => {
            if state.keys.selected + 1 < snap.keys.len() {
                state.keys.selected += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') if state.screen == RunScreen::Keys => {
            state.keys.selected = state.keys.selected.saturating_sub(1);
        }
        KeyCode::Char('a') if state.screen == RunScreen::Keys => {
            state.mode = InputMode::AddKeyLabel { buf: String::new() };
        }
        KeyCode::Char('r') if state.screen == RunScreen::Keys => {
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if let (Some(bridge), Some(cid)) = (deps.admin_bridge, state.resources.config_id)
            {
                if let Some(key_id) = snap.keys.get(state.keys.selected).map(|k| k.key_id) {
                    submit_inline_admin_job(state, bridge, AdminTaskKind::RotatingKey, |c| {
                        AdminJob::RotateApiKey {
                            corr: c,
                            config_id: cid,
                            key_id,
                        }
                    });
                }
            }
        }
        KeyCode::Char('d') if state.screen == RunScreen::Keys => {
            if let Some(key_id) = snap.keys.get(state.keys.selected).map(|k| k.key_id) {
                state.mode = InputMode::ConfirmKeyRevoke { key_id };
                state.status_msg = "Press y to confirm revoke (Esc cancels)".into();
            }
        }
        KeyCode::Char('y')
            if state.screen == RunScreen::Keys
                && matches!(state.mode, InputMode::ConfirmKeyRevoke { .. }) =>
        {
            let key_id = match std::mem::replace(&mut state.mode, InputMode::Normal) {
                InputMode::ConfirmKeyRevoke { key_id } => key_id,
                _ => return false,
            };
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if let (Some(bridge), Some(cid)) = (deps.admin_bridge, state.resources.config_id)
            {
                submit_inline_admin_job(state, bridge, AdminTaskKind::RevokingKey, |c| {
                    AdminJob::RevokeApiKey {
                        corr: c,
                        config_id: cid,
                        key_id,
                    }
                });
            }
        }
        KeyCode::Char('c')
            if state.screen == RunScreen::Keys
                && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            if state.admin_busy() {
                state.status_msg = "Busy — wait for the current admin task.".into();
            } else if let (Some(bridge), Some(cid)) = (deps.admin_bridge, state.resources.config_id)
            {
                if let Some(key_id) = snap.keys.get(state.keys.selected).map(|k| k.key_id) {
                    submit_inline_admin_job(state, bridge, AdminTaskKind::RevealingKey, |c| {
                        AdminJob::RevealApiKey {
                            corr: c,
                            config_id: cid,
                            key_id,
                        }
                    });
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') if state.screen == RunScreen::Logs => {
            let inner_h = 20usize;
            let visible_rows = inner_h.max(1);
            let total = state.logs.lines.len();
            let max_top = total.saturating_sub(visible_rows.min(total.max(1)));
            if state.logs.scroll < max_top {
                state.logs.scroll += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') if state.screen == RunScreen::Logs => {
            state.logs.scroll = state.logs.scroll.saturating_sub(1);
        }
        KeyCode::PageDown if state.screen == RunScreen::Logs => {
            let page = 20usize;
            let total = state.logs.lines.len();
            let max_top = total.saturating_sub(1);
            state.logs.scroll = (state.logs.scroll + page).min(max_top);
        }
        KeyCode::PageUp if state.screen == RunScreen::Logs => {
            let page = 20usize;
            state.logs.scroll = state.logs.scroll.saturating_sub(page);
        }
        KeyCode::Char('g') if state.screen == RunScreen::Logs => {
            state.logs.scroll = 0;
        }
        KeyCode::Char('G') if state.screen == RunScreen::Logs => {
            state.logs.scroll = state.logs.lines.len().saturating_sub(1);
        }
        _ => {}
    }
    false
}

fn update(state: &mut RunState, msg: UiMsg, deps: &UpdateDeps<'_>) -> bool {
    match msg {
        UiMsg::Tick => {
            state.reset_screen_local_mode();
            false
        }
        UiMsg::Admin(comp) => {
            apply_admin_completion(state, deps.admin_bridge, comp);
            false
        }
        UiMsg::LogLine(line) => {
            state.logs.lines.push_back(line);
            while state.logs.lines.len() > appliance_log::APPLIANCE_LOG_TAB_MAX_LINES {
                state.logs.lines.pop_front();
            }
            false
        }
        UiMsg::Key(key) => {
            state.show_help = false;
            match state.mode {
                InputMode::ApiFilter
                | InputMode::AddKeyLabel { .. }
                | InputMode::OAuthWizard(_) => update_modal_key(state, key, deps),
                InputMode::Normal
                | InputMode::ConfirmOAuthDisable { .. }
                | InputMode::ConfirmKeyRevoke { .. } => update_normal_key(state, key, deps),
            }
        }
    }
}

fn render_running_frame(
    frame: &mut ratatui::Frame<'_>,
    model: &RunState,
    host_state: &PlasmHostState,
    http_port: u16,
    mcp_port: u16,
) {
    let snap = &model.resources.snapshot;
    let tab_titles: Vec<&str> = RunScreen::ALL.iter().map(|s| s.title()).collect();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let title = Paragraph::new(Line::from(vec![
        Span::styled("[RUN]", run_badge_style()),
        Span::styled(" PLASM APPLIANCE ", run_title_style()),
        Span::raw(format!(
            "  HTTP :{}  MCP :{}  ? help  q quit",
            http_port, mcp_port
        )),
    ]))
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    let tab_spans: Vec<Span> = tab_titles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if i == model.screen.index() {
                let mut st = Style::default().add_modifier(Modifier::BOLD);
                if !no_color() {
                    st = st.fg(Color::Black).bg(Color::Yellow);
                }
                Span::styled(format!(" [{t}] "), st)
            } else {
                let mut st = Style::default();
                if !no_color() {
                    st = st.fg(Color::DarkGray);
                } else {
                    st = st.add_modifier(Modifier::DIM);
                }
                Span::styled(format!(" {t} "), st)
            }
        })
        .collect();
    let tab_bar =
        Paragraph::new(Line::from(tab_spans)).block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(tab_bar, chunks[1]);

    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(model.screen.title());

    match model.screen {
        RunScreen::Status => {
            let scope = appliance_mcp_scope();
            let mut lines = vec![
                Line::from("Listeners"),
                Line::from(format!("  HTTP   http://127.0.0.1:{http_port}")),
                Line::from(format!("  MCP    http://127.0.0.1:{mcp_port}/mcp")),
                Line::from(""),
                Line::from("Your MCP (singleton)"),
            ];
            match &snap.config_surface {
                McpConfigSurfaceState::Ready {
                    summary_name,
                    summary_status,
                    enabled_api_count,
                    key_count,
                } => {
                    lines.push(Line::from("  policy store (project_mcp_*): enabled"));
                    lines.push(Line::from(format!(
                        "  tenant / workspace / project: {} / {} / {}",
                        scope.tenant_id, scope.workspace_slug, scope.project_slug
                    )));
                    lines.push(Line::from(format!(
                        "  config: {}  ({})",
                        summary_name, summary_status
                    )));
                    lines.push(Line::from(format!(
                        "  enabled APIs: {}  transport keys: {}",
                        enabled_api_count, key_count
                    )));
                    if let Some(id) = model.resources.config_id {
                        lines.push(Line::from(format!("  config_id: {id}")));
                    }
                }
                McpConfigSurfaceState::ConfigLoadError => {
                    lines.push(Line::from(vec![
                        Span::styled("  ⚠ ", err_emphasis_style()),
                        Span::styled(
                            "MCP policy store online, but the singleton config failed to load.",
                            err_emphasis_style(),
                        ),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  ➜ ", dim_style()),
                        Span::raw("Wait for refresh or inspect startup / DB diagnostics."),
                    ]));
                }
                McpConfigSurfaceState::PolicyStoreUnavailable => {
                    lines.push(Line::from(vec![
                        Span::styled("  ⧗ ", err_emphasis_style()),
                        Span::styled("ERROR — MCP policy store offline", err_emphasis_style()),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  ⚠ ", err_emphasis_style()),
                        Span::styled(
                            "project_mcp_* not reachable (database missing or migrations failed).",
                            err_emphasis_style(),
                        ),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("  ⊗ ", err_emphasis_style()),
                        Span::styled(
                            "Transport API keys and API allowlists are disabled until this is fixed.",
                            err_emphasis_style(),
                        ),
                    ]));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("  ➜ ", dim_style()),
                        Span::raw("tenant / workspace / project: "),
                        Span::styled(
                            format!(
                                "{} / {} / {}",
                                scope.tenant_id, scope.workspace_slug, scope.project_slug
                            ),
                            dim_style(),
                        ),
                    ]));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("  ➜ ", dim_style()),
                        Span::raw("Try: "),
                        Span::styled(
                            "plasm-appliance mcp migrate-db",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            " — embedded Postgres autostart + Storage tab for URLs / disk.",
                            dim_style(),
                        ),
                    ]));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "Trace hub: {}",
                plasm_agent_core::appliance_services::trace_hub_bounds_summary(host_state)
            )));
            if !model.status_msg.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(format!("Last action: {}", model.status_msg)));
            }
            frame.render_widget(Paragraph::new(lines).block(main_block), chunks[2]);
        }
        RunScreen::Clients => {
            let url = mcp_streamable_url(mcp_port);
            let curl = mcp_curl_snippet(mcp_port);
            let accent = if no_color() {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Cyan)
            };
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(
                        "MCP endpoint",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  # URL", dim_style()),
                    Span::raw("  "),
                    Span::styled("% curl", dim_style()),
                ]),
                Line::from(vec![Span::styled(format!("  {url}"), accent)]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Authorization",
                    Style::default().add_modifier(Modifier::BOLD),
                )]),
                Line::from("  Header: Authorization: Bearer <api_key>"),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "curl (generic)",
                    Style::default().add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![Span::styled(format!("  {curl}"), dim_style())]),
            ];
            if let Some(sel) = snap.keys.get(model.keys.selected) {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled(
                        "Selected key",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  (for your notes — use ", dim_style()),
                    Span::styled("c", dim_style().add_modifier(Modifier::BOLD)),
                    Span::styled(" reveal on Keys tab for secret)", dim_style()),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(api_key_row_label(sel), accent),
                    Span::styled(format!("   {}", sel.key_id), dim_style()),
                ]));
            } else {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("No keys yet", dim_style()),
                    Span::raw(" — add one on the "),
                    Span::styled("Keys", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" tab."),
                ]));
            }
            frame.render_widget(
                Paragraph::new(lines).block(main_block.title("Clients")),
                chunks[2],
            );
        }
        RunScreen::Apis => {
            let mut filter_line = format!("Filter: {}", model.api.filter);
            if matches!(model.mode, InputMode::ApiFilter) {
                filter_line.push('_');
            }
            let mut lines = vec![
                Line::from(filter_line),
                Line::from("Space toggle  s save staged  / filter  ·  Enter Tab Esc close filter"),
                Line::from(""),
            ];
            for (fi, &row_ix) in model.api.filtered_ix.iter().enumerate() {
                let r = &snap.catalog_rows[row_ix];
                let on = row_enabled(model, snap, &r.entry_id);
                let mark = if on { "[on]" } else { "[off]" };
                let mark_style = if on {
                    api_toggle_on_style()
                } else {
                    api_toggle_off_style()
                };
                let selected = fi == model.api.selected;
                let mut name_style = if on {
                    Style::default()
                } else {
                    api_toggle_off_style()
                };
                if selected {
                    name_style = name_style.add_modifier(Modifier::BOLD);
                }
                let mut auth_style = dim_style();
                if selected {
                    auth_style = auth_style.add_modifier(Modifier::BOLD);
                }
                let name = catalog_row_display_name(&r.entry_id, &r.label);
                lines.push(Line::from(vec![
                    Span::styled(mark, mark_style),
                    Span::raw(" "),
                    Span::styled(name, name_style),
                    Span::raw("  "),
                    Span::styled(format!("{:?}", r.auth_marker), auth_style),
                ]));
            }
            frame.render_widget(
                Paragraph::new(lines).block(main_block.title("APIs")),
                chunks[2],
            );
        }
        RunScreen::OAuth => {
            if let InputMode::OAuthWizard(ref wiz) = model.mode {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(
                            "New OAuth provider (upsert) ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("Esc cancel", dim_style()),
                        Span::raw(" · "),
                        Span::styled("Enter", dim_style()),
                        Span::raw(" next / confirm"),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Field: ", dim_style()),
                        Span::styled(
                            wiz.prompt_title(),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                ];
                match wiz.step {
                    OAuthUpsertStep::EntryId => {
                        let mut search = wiz.buf.clone();
                        search.push('_');
                        lines.push(Line::from(vec![
                            Span::styled("Search: ", dim_style()),
                            Span::styled(search, Style::default().add_modifier(Modifier::BOLD)),
                        ]));
                        lines.push(Line::from(Span::styled(
                            "Type to filter · ↑↓ or j/k choose · Enter selects",
                            dim_style(),
                        )));
                        lines.push(Line::from(""));
                        let matches = wiz.filtered_entry_indices(&snap.catalog_rows);
                        if matches.is_empty() {
                            lines.push(Line::from(Span::styled(
                                "(No registry API matches the current search.)",
                                dim_style(),
                            )));
                        } else {
                            let selected = wiz.entry_sel.min(matches.len().saturating_sub(1));
                            let start = selected.saturating_sub(4);
                            let end = (start + 8).min(matches.len());
                            let start = end.saturating_sub(8);
                            if start > 0 {
                                lines.push(Line::from(Span::styled(
                                    format!("… {} earlier matches", start),
                                    dim_style(),
                                )));
                            }
                            for (offset, row_ix) in matches[start..end].iter().enumerate() {
                                let absolute = start + offset;
                                let row = &snap.catalog_rows[*row_ix];
                                let picked = absolute == selected;
                                let mut row_style = Style::default();
                                let mut meta_style = dim_style();
                                if picked {
                                    row_style = row_style.add_modifier(Modifier::BOLD);
                                    meta_style = meta_style.add_modifier(Modifier::BOLD);
                                }
                                lines.push(Line::from(vec![
                                    Span::styled(if picked { "› " } else { "  " }, row_style),
                                    Span::styled(
                                        catalog_row_display_name(&row.entry_id, &row.label),
                                        row_style,
                                    ),
                                    Span::raw("  "),
                                    Span::styled(format!("{:?}", row.auth_marker), meta_style),
                                ]));
                            }
                            if end < matches.len() {
                                lines.push(Line::from(Span::styled(
                                    format!("… {} more matches", matches.len() - end),
                                    dim_style(),
                                )));
                            }
                        }
                    }
                    OAuthUpsertStep::Enabled => {
                        lines.push(Line::from(vec![
                            Span::raw("enabled: "),
                            Span::styled(
                                if wiz.enabled { "yes" } else { "no" },
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                            Span::styled("  — Space toggles, Enter review", dim_style()),
                        ]));
                    }
                    OAuthUpsertStep::Confirm => {
                        lines.push(Line::from(vec![Span::styled(
                            "Review — Enter save, Esc cancel wizard",
                            dim_style(),
                        )]));
                        lines.push(Line::from(""));
                        for s in wiz.summary_lines() {
                            lines.push(Line::from(Span::raw(s)));
                        }
                    }
                    _ => {
                        let mut edit = wiz.buf.clone();
                        edit.push('_');
                        lines.push(Line::from(vec![Span::styled(
                            edit,
                            Style::default().add_modifier(Modifier::BOLD),
                        )]));
                    }
                }
                if !model.status_msg.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!("Last action: {}", model.status_msg)));
                }
                frame.render_widget(
                    Paragraph::new(lines).block(main_block.title("OAuth — new provider")),
                    chunks[2],
                );
            } else {
                let mut lines = vec![
                    Line::from("Connected APIs (OAuth)"),
                    Line::from(
                        "↑↓ or j/k — select   n new provider   d device bind   x disable + y confirm",
                    ),
                    Line::from(vec![
                        Span::styled("Tip: ", dim_style()),
                        Span::styled(
                            "plasm-appliance oauth",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" for scripting / secrets via stdin.", dim_style()),
                    ]),
                    Line::from(""),
                ];
                if let Some(status) = oauth_surface_status(snap) {
                    lines.push(Line::from(vec![
                        Span::styled("OAuth status: ", err_emphasis_style()),
                        Span::styled(status, err_emphasis_style()),
                    ]));
                    lines.push(Line::from(""));
                }
                if snap.oauth_surface.provider_store_ready() && snap.oauth_providers.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("(No providers — press ", dim_style()),
                        Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(" here or use ", dim_style()),
                        Span::styled(
                            "plasm-appliance oauth",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(".)", dim_style()),
                    ]));
                }
                for (i, row) in snap.oauth_providers.iter().enumerate() {
                    let sel = i == model.oauth.selected;
                    let mut row_style = Style::default();
                    if sel {
                        row_style = row_style.add_modifier(Modifier::BOLD);
                    }
                    let hint = snap
                        .oauth_binding_hints
                        .get(i)
                        .map(String::as_str)
                        .unwrap_or("?");
                    let device_ep = row.device_authorization_endpoint.as_deref().unwrap_or("");
                    lines.push(Line::from(vec![
                        Span::styled(if sel { "› " } else { "  " }, row_style),
                        Span::styled(row.entry_id.as_str(), row_style),
                        Span::raw(format!(
                            "  en={}  device_ep={}  {}",
                            row.enabled, device_ep, hint
                        )),
                    ]));
                }
                if model.pending_oauth_disable_entry().is_some() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("Disable pending — press ", err_emphasis_style()),
                        Span::styled("y", err_emphasis_style().add_modifier(Modifier::BOLD)),
                        Span::styled(" to confirm, Esc cancel", err_emphasis_style()),
                    ]));
                }
                if !model.status_msg.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!("Last action: {}", model.status_msg)));
                }
                frame.render_widget(
                    Paragraph::new(lines).block(main_block.title("OAuth")),
                    chunks[2],
                );
            }
        }
        RunScreen::Keys => {
            let items: Vec<ListItem> = snap
                .keys
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    let style = if i == model.keys.selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    let dim = dim_style();
                    ListItem::new(Line::from(vec![
                        Span::styled(api_key_row_label(k), style),
                        Span::styled("  ·  ", dim),
                        Span::styled(k.key_id.to_string(), dim),
                    ]))
                })
                .collect();
            let mut hint = vec![
                Line::from(vec![
                    Span::raw("a add   r rotate   d revoke + y confirm   c reveal → status   "),
                    Span::styled("#", dim_style()),
                    Span::raw(" copy row"),
                ]),
                Line::from(""),
            ];
            let hint_head_lines: u16 = if let Some(buf) = model.add_key_label_buf() {
                hint.push(Line::from(vec![Span::styled(
                    format!("New key label: {buf}_"),
                    Style::default().add_modifier(Modifier::BOLD),
                )]));
                hint.push(Line::from(vec![
                    Span::styled("Enter ", dim_style()),
                    Span::raw("confirm · "),
                    Span::styled("Esc ", dim_style()),
                    Span::raw("cancel · "),
                    Span::styled("^C ", dim_style()),
                    Span::raw("quit appliance"),
                ]));
                hint.push(Line::from(""));
                6
            } else {
                2
            };
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(hint_head_lines.min(chunks[2].height.max(1))),
                    Constraint::Min(0),
                ])
                .split(chunks[2]);
            frame.render_widget(Paragraph::new(hint), split[0]);
            frame.render_widget(List::new(items).block(main_block), split[1]);
        }
        RunScreen::Runs => {
            let lines = vec![
                Line::from("Runs / traces"),
                Line::from(""),
                Line::from("Operational drill-down binds to execute session store and trace hub."),
                Line::from("Strict remote client: plasm-cgs (transport-only)."),
            ];
            frame.render_widget(Paragraph::new(lines).block(main_block), chunks[2]);
        }
        RunScreen::Storage => {
            let db = std::env::var("DATABASE_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .map(|_| "(set)")
                .unwrap_or("(unset)");
            let emb = std::env::var("PLASM_EMBEDDED_POSTGRES")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "(unset — autostart)".into());
            let lines = vec![
                Line::from("Storage / Postgres"),
                Line::from(format!("  DATABASE_URL            {db}")),
                Line::from(format!("  PLASM_EMBEDDED_POSTGRES {emb}")),
                Line::from(""),
                Line::from(
                    "Embedded Postgres (pg-embed, PostgreSQL 15) starts by default: cache data dir, port 55432, DB plasm_appliance, superuser password plasm_embedded_local_dev if unset (pg-embed initdb requires non-empty pwfile). Opt out: PLASM_EMBEDDED_POSTGRES=0. Override: PGDATA / PLASM_EMBEDDED_POSTGRES_* / DATABASE_URL (loopback).",
                ),
            ];
            frame.render_widget(Paragraph::new(lines).block(main_block), chunks[2]);
        }
        RunScreen::Logs => {
            let inner_h = chunks[2].height.saturating_sub(2) as usize;
            let visible_rows = inner_h.max(1);
            let total = model.logs.lines.len();
            let max_top = total.saturating_sub(visible_rows.min(total.max(1)));
            let top = model.logs.scroll.min(max_top);
            let items: Vec<ListItem> = model
                .logs
                .lines
                .iter()
                .skip(top)
                .take(visible_rows)
                .map(|s: &String| ListItem::new(Line::from(Span::raw(s.as_str()))))
                .collect();
            frame.render_widget(
                List::new(items)
                    .block(main_block.title("Logs — tracing + HTTP help · ↑↓ j/k PgUp/Dn g/G")),
                chunks[2],
            );
        }
    }

    let mut footer_spans = vec![
        Span::styled("←/→", dim_style()),
        Span::raw(" tab  "),
        Span::styled("?", dim_style()),
        Span::raw(" help  "),
        Span::styled("q", dim_style()),
        Span::raw(" quit"),
    ];
    if model.screen == RunScreen::Clients {
        footer_spans.push(Span::raw("  |  "));
        footer_spans.push(Span::styled("#", dim_style()));
        footer_spans.push(Span::raw(" URL  "));
        footer_spans.push(Span::styled("%", dim_style()));
        footer_spans.push(Span::raw(" curl"));
    }
    if model.screen == RunScreen::Keys && model.add_key_label_buf().is_none() {
        footer_spans.push(Span::raw("  |  "));
        footer_spans.push(Span::styled("#", dim_style()));
        footer_spans.push(Span::raw(" copy row"));
    }
    if model.show_help {
        footer_spans.push(Span::raw("  |  "));
        footer_spans.push(Span::raw(
            "Clients: # URL % curl · APIs: / Space s · OAuth: n d x+y · Keys: a r d c # · ^C quit · Logs: ↑↓ PgUp/Dn g/G",
        ));
    }
    if let Some(task) = model.resources.admin.busy_task() {
        footer_spans.push(Span::raw("  |  "));
        let label = task.kind.label();
        let hint = format!("{label} {:.0}s", task.started_at.elapsed().as_secs_f32());
        footer_spans.push(Span::styled(hint, dim_style()));
    }
    let footer =
        Paragraph::new(Line::from(footer_spans)).block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, chunks[3]);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_running_mode(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    host_state: Arc<PlasmHostState>,
    running: Arc<AtomicBool>,
    ui_evt_tx: Option<Sender<UiEvent>>,
    http_port: u16,
    mcp_port: u16,
    admin_bridge: Option<AdminBridge>,
    log_rx: Option<crossbeam_channel::Receiver<String>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut run_entered_sent = false;
    let mut model = RunState::new();
    if let Some(ref bridge) = admin_bridge {
        enqueue_refresh_if_idle(&mut model, bridge);
    }
    let deps = UpdateDeps {
        admin_bridge: admin_bridge.as_ref(),
        host_state: Some(host_state.as_ref()),
        mcp_port,
    };

    while running.load(Ordering::SeqCst) {
        if let Some(ref lr) = log_rx {
            for _ in 0..512 {
                match lr.try_recv() {
                    Ok(line) => {
                        let _ = update(&mut model, UiMsg::LogLine(line), &deps);
                    }
                    Err(_) => break,
                }
            }
        }
        if let Some(ref bridge) = admin_bridge {
            while let Ok(comp) = bridge.completions().try_recv() {
                let _ = update(&mut model, UiMsg::Admin(comp), &deps);
            }
        } else if matches!(
            model.resources.snapshot.config_surface,
            McpConfigSurfaceState::PolicyStoreUnavailable
        ) && appliance_services_policy_hint(host_state.as_ref())
        {
            model.status_msg = "Waiting for admin bridge / policy store…".into();
        }
        let _ = update(&mut model, UiMsg::Tick, &deps);

        terminal.draw(|frame| {
            render_running_frame(frame, &model, host_state.as_ref(), http_port, mcp_port)
        })?;

        if !run_entered_sent {
            if let Some(ref tx) = ui_evt_tx {
                let _ = tx.send(UiEvent::RunEntered);
            }
            run_entered_sent = true;
        }

        if event::poll(Duration::from_millis(120))? {
            if let Event::Key(key) = event::read()? {
                if raw_tty_wants_process_quit(&key) {
                    running.store(false, Ordering::SeqCst);
                    break;
                }
                if update(&mut model, UiMsg::Key(key), &deps) {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn appliance_services_policy_hint(state: &PlasmHostState) -> bool {
    plasm_agent_core::appliance_services::mcp_policy_store_enabled(state)
}

/// Alternate-screen RUN UI only (no BOOT checklist).
#[allow(dead_code)]
pub fn run_control_station(
    state: Arc<PlasmHostState>,
    running: Arc<AtomicBool>,
    http_port: u16,
    mcp_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    enable_raw_mode()?;
    let mut buffer = stdout();
    execute!(buffer, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(buffer);
    let mut terminal = Terminal::new(backend)?;

    let restore_terminal = || {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    };
    let _guard = scopeguard::guard((), |_| restore_terminal());

    let result = run_running_mode(
        &mut terminal,
        state,
        running,
        None,
        http_port,
        mcp_port,
        None,
        None,
    );

    drop(_guard);
    let _ = terminal.show_cursor();

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn test_deps<'a>(bridge: Option<&'a AdminBridge>) -> UpdateDeps<'a> {
        UpdateDeps {
            admin_bridge: bridge,
            host_state: None,
            mcp_port: 4100,
        }
    }

    fn sample_oauth_provider(
        entry_id: &str,
    ) -> plasm_agent_core::oauth_provider_repository::OauthProviderAppRow {
        plasm_agent_core::oauth_provider_repository::OauthProviderAppRow {
            entry_id: entry_id.into(),
            authorization_endpoint: Some("https://example.test/authorize".into()),
            token_endpoint: Some("https://example.test/token".into()),
            device_authorization_endpoint: Some("https://example.test/device".into()),
            client_id: "client-id".into(),
            client_secret_key: "kv/key".into(),
            enabled: true,
        }
    }

    #[test]
    fn run_screen_wraps_left_and_right() {
        let mut state = RunState::new();
        let deps = test_deps(None);

        assert!(matches!(state.screen, RunScreen::Status));
        assert!(!update(&mut state, UiMsg::Key(key(KeyCode::Left)), &deps));
        assert!(matches!(state.screen, RunScreen::Logs));
        assert!(!update(&mut state, UiMsg::Key(key(KeyCode::Right)), &deps));
        assert!(matches!(state.screen, RunScreen::Status));
    }

    #[test]
    fn api_filter_mode_enters_and_esc_clears() {
        let mut state = RunState::new();
        state.screen = RunScreen::Apis;
        let deps = test_deps(None);

        update(&mut state, UiMsg::Key(key(KeyCode::Char('/'))), &deps);
        assert!(matches!(state.mode, InputMode::ApiFilter));

        update(&mut state, UiMsg::Key(key(KeyCode::Char('g'))), &deps);
        assert_eq!(state.api.filter, "g");

        update(&mut state, UiMsg::Key(key(KeyCode::Esc)), &deps);
        assert!(matches!(state.mode, InputMode::Normal));
        assert!(state.api.filter.is_empty());
    }

    #[test]
    fn add_key_modal_confirms_and_cancels() {
        let mut state = RunState::new();
        state.screen = RunScreen::Keys;
        let deps = test_deps(None);

        update(&mut state, UiMsg::Key(key(KeyCode::Char('a'))), &deps);
        assert!(matches!(state.mode, InputMode::AddKeyLabel { .. }));

        update(&mut state, UiMsg::Key(key(KeyCode::Char('x'))), &deps);
        assert_eq!(state.add_key_label_buf(), Some("x"));

        update(&mut state, UiMsg::Key(key(KeyCode::Esc)), &deps);
        assert!(matches!(state.mode, InputMode::Normal));

        update(&mut state, UiMsg::Key(key(KeyCode::Char('a'))), &deps);
        update(&mut state, UiMsg::Key(key(KeyCode::Char('y'))), &deps);
        update(&mut state, UiMsg::Key(key(KeyCode::Enter)), &deps);
        assert!(matches!(state.mode, InputMode::Normal));
    }

    #[test]
    fn oauth_disable_confirm_cancels_cleanly() {
        let mut state = RunState::new();
        state.screen = RunScreen::OAuth;
        state.resources.snapshot.oauth_providers = vec![sample_oauth_provider("github")];
        let deps = test_deps(None);

        update(&mut state, UiMsg::Key(key(KeyCode::Char('x'))), &deps);
        assert!(matches!(state.mode, InputMode::ConfirmOAuthDisable { .. }));

        update(&mut state, UiMsg::Key(key(KeyCode::Esc)), &deps);
        assert!(matches!(state.mode, InputMode::Normal));
        assert_eq!(state.status_msg, "OAuth disable cancelled.");
    }

    #[test]
    fn stale_refresh_completion_is_ignored() {
        let mut state = RunState::new();
        state.resources.snapshot.config_surface = McpConfigSurfaceState::Ready {
            summary_name: "old".into(),
            summary_status: "old-status".into(),
            enabled_api_count: 0,
            key_count: 0,
        };
        state.resources.admin.start_refresh(7);
        let deps = test_deps(None);

        let data = RefreshedUiData {
            config_surface: McpConfigSurfaceState::Ready {
                summary_name: "new".into(),
                summary_status: "ready".into(),
                enabled_api_count: 1,
                key_count: 0,
            },
            config_id: Some(Uuid::nil()),
            catalog_rows: Vec::new(),
            keys: Vec::new(),
            db_allowed: HashSet::new(),
            oauth_providers: Vec::new(),
            oauth_binding_hints: Vec::new(),
            oauth_surface: OAuthSurfaceState::CatalogUnavailable,
        };

        update(
            &mut state,
            UiMsg::Admin(AdminCompletion::RefreshFull { corr: 6, data }),
            &deps,
        );

        assert!(matches!(
            state.resources.snapshot.config_surface,
            McpConfigSurfaceState::Ready { ref summary_name, .. } if summary_name == "old"
        ));
        assert_eq!(state.resources.admin.pending_refresh_corr(), Some(7));
    }
}
