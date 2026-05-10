//! Remote HTTP terminal for `plasm-cgs`: discovery, execute sessions, plan/run, mirror hooks.
//!
//! See `docs/plasm-cgs-remote-terminal.md` in the parent repo.

use anyhow::{anyhow, Context as _, Result};
use clap::{Parser, Subcommand};
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE,
};
use reqwest::redirect::Policy;
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use crate::http_discovery::TerminalDiscoverBody;
use crate::http_execute::{CapabilitySeed, CreateExecuteSessionBody, ExecuteSessionContextBody};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TerminalProfile {
    server: Option<String>,
    api_key: Option<String>,
    bearer_token: Option<String>,
}

#[derive(Parser)]
#[command(
    name = "plasm-cgs",
    about = "Remote Plasm terminal (HTTP execute / discovery protocol)"
)]
struct Cli {
    /// Plasm HTTP server origin (e.g. http://127.0.0.1:3000). Overrides profile / `PLASM_CGS_SERVER`.
    /// Overrides `PLASM_CGS_SERVER` and profile `server`.
    #[arg(long, global = true)]
    server: Option<String>,
    /// Profile name under `~/.plasm/cgs/profiles/<name>.json`.
    #[arg(long, global = true, default_value = "default")]
    profile: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Verify inbound auth against `GET /v1/incoming-auth/context`.
    Whoami,
    /// Intent-native discovery (`POST /v1/terminal/discover`).
    Search {
        /// Natural-language intent (quote words if needed).
        #[arg(required = true)]
        intent: String,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Open execute session (`POST /execute` → follow 303, then `GET` session JSON).
    Open {
        #[arg(long)]
        entry: String,
        #[arg(long, value_delimiter = ',')]
        entities: Vec<String>,
        #[arg(long)]
        intent: Option<String>,
    },
    /// Append federated / expanded seeds (`POST /execute/{prompt_hash}/{session}/context`).
    Context {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
        /// `entry_id:entity` pairs (e.g. `--seed overshow:Profile`, repeatable).
        #[arg(long = "seed", action = clap::ArgAction::Append)]
        seeds: Vec<String>,
        #[arg(long)]
        intent: Option<String>,
    },
    /// Run Plasm lines (`POST /execute/...`); body from stdin unless `--file`.
    Run {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
        /// `plan` or `run` (also `X-Plasm-Run-Mode`).
        #[arg(long, default_value = "run")]
        mode: String,
        #[arg(long, default_value = "text/toon")]
        accept: String,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Structured symbols (`GET /execute/.../symbols`).
    Symbols {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
    },
    /// Session status (`GET /execute/.../status`).
    Status {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
    },
    /// Run index from session hot cache (`GET /execute/.../runs`).
    Runs {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
    },
    /// Fetch run artifact JSON (`GET /execute/.../artifacts/{run_id}`).
    Artifact {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
        #[arg(long)]
        run_id: String,
    },
    /// Store API key / bearer in profile JSON (optional helper).
    AuthSet {
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        bearer_token: Option<String>,
    },
    /// Print local mirror directory for a session (creates layout).
    MirrorPath {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
    },
    /// Repair local mirror from server run index + artifact GETs.
    MirrorPull {
        #[arg(long)]
        prompt_hash: String,
        #[arg(long)]
        session: String,
    },
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn profile_path(name: &str) -> PathBuf {
    home_dir()
        .join(".plasm/cgs/profiles")
        .join(format!("{name}.json"))
}

fn load_profile(name: &str) -> Result<TerminalProfile> {
    let p = profile_path(name);
    if !p.exists() {
        return Ok(TerminalProfile::default());
    }
    let raw =
        std::fs::read_to_string(&p).with_context(|| format!("read profile {}", p.display()))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn save_profile(name: &str, prof: &TerminalProfile) -> Result<()> {
    let p = profile_path(name);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(prof)?)?;
    Ok(())
}

fn resolve_server(cli: Option<&str>, profile: &TerminalProfile) -> Result<String> {
    if let Some(s) = cli {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.trim_end_matches('/').to_string());
        }
    }
    if let Ok(s) = std::env::var("PLASM_CGS_SERVER") {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.trim_end_matches('/').to_string());
        }
    }
    if let Some(ref s) = profile.server {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.trim_end_matches('/').to_string());
        }
    }
    Err(anyhow!(
        "missing server: pass --server, set PLASM_CGS_SERVER, or add \"server\" to the profile JSON"
    ))
}

fn resolve_api_key(profile: &TerminalProfile) -> Option<String> {
    std::env::var("PLASM_CGS_API_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            profile
                .api_key
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

fn resolve_bearer(profile: &TerminalProfile) -> Option<String> {
    std::env::var("PLASM_CGS_BEARER_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            profile
                .bearer_token
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

fn apply_auth_headers(headers: &mut HeaderMap, profile: &TerminalProfile) -> Result<()> {
    let api_key = resolve_api_key(profile);
    let bearer = resolve_bearer(profile);
    match (api_key, bearer) {
        (Some(k), _) => {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(k.trim())
                    .map_err(|e| anyhow!("invalid API key header: {e}"))?,
            );
        }
        (None, Some(tok)) => {
            let v = format!("Bearer {}", tok.trim());
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&v).map_err(|e| anyhow!("invalid bearer header: {e}"))?,
            );
        }
        (None, None) => {}
    }
    Ok(())
}

fn server_slug(server: &str) -> String {
    let h = Sha256::digest(server.as_bytes());
    hex::encode(h)[..12].to_string()
}

fn session_mirror_dir(server: &str, prompt_hash: &str, session: &str) -> PathBuf {
    home_dir()
        .join(".plasm/cgs/servers")
        .join(server_slug(server))
        .join("sessions")
        .join(prompt_hash)
        .join(session)
}

fn write_mirror_meta(
    server: &str,
    prompt_hash: &str,
    session: &str,
    label: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    let dir = session_mirror_dir(server, prompt_hash, session);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(label);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

fn read_program_body(path: Option<&PathBuf>) -> Result<Vec<u8>> {
    if let Some(p) = path {
        std::fs::read(p).with_context(|| format!("read {}", p.display()))
    } else {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        Ok(buf)
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_bytes(
    client: &Client,
    server: &str,
    profile: &TerminalProfile,
    method: Method,
    path: &str,
    accept: Option<&str>,
    content_type: Option<&str>,
    body: Option<Vec<u8>>,
) -> Result<(StatusCode, HeaderMap, Vec<u8>)> {
    let url = format!(
        "{}/{}",
        server.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let mut req = client.request(method, url);
    let mut headers = HeaderMap::new();
    apply_auth_headers(&mut headers, profile)?;
    if let Some(a) = accept {
        headers.insert(ACCEPT, HeaderValue::from_str(a)?);
    }
    if let Some(ct) = content_type {
        headers.insert(CONTENT_TYPE, HeaderValue::from_str(ct)?);
    }
    if let Some(ref b) = body {
        headers.insert(CONTENT_LENGTH, HeaderValue::from(b.len()));
        req = req.headers(headers).body(b.clone());
    } else {
        req = req.headers(headers);
    }
    let res = req.send().await?;
    let status = res.status();
    let hdrs = res.headers().clone();
    let bytes = res.bytes().await?.to_vec();
    Ok((status, hdrs, bytes))
}

/// Entry point for the `plasm-cgs` binary.
pub async fn run_terminal() -> Result<()> {
    crate::init_agent_runtime().map_err(|e| anyhow!("{e}"))?;
    let cli = Cli::parse();
    let mut profile = load_profile(cli.profile.as_str())?;
    let server = resolve_server(cli.server.as_deref(), &profile)?;
    let client = Client::builder()
        .build()
        .map_err(|e| anyhow!("http client: {e}"))?;

    match cli.cmd {
        Cmd::AuthSet {
            api_key,
            bearer_token,
        } => {
            if api_key.is_none() && bearer_token.is_none() {
                return Err(anyhow!("auth-set: pass --api-key and/or --bearer-token"));
            }
            if let Some(k) = api_key {
                profile.api_key = Some(k);
            }
            if let Some(b) = bearer_token {
                profile.bearer_token = Some(b);
            }
            save_profile(cli.profile.as_str(), &profile)?;
            println!(
                "updated profile {}",
                profile_path(cli.profile.as_str()).display()
            );
            Ok(())
        }
        Cmd::Whoami => {
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::GET,
                "/v1/incoming-auth/context",
                Some("application/json"),
                None,
                None,
            )
            .await?;
            if !st.is_success() {
                eprintln!("whoami: HTTP {}", st);
                std::io::stdout().write_all(&body)?;
                std::process::exit(1);
            }
            std::io::stdout().write_all(&body)?;
            println!();
            Ok(())
        }
        Cmd::Search { intent, limit } => {
            let utterance = intent.trim().to_string();
            if utterance.is_empty() {
                return Err(anyhow!("search: intent text required"));
            }
            let payload = serde_json::to_vec(&TerminalDiscoverBody {
                intent: utterance,
                limit,
                allowed_entry_ids: vec![],
                enable_embeddings: false,
            })?;
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::POST,
                "/v1/terminal/discover",
                Some("application/json"),
                Some("application/json"),
                Some(payload),
            )
            .await?;
            if !st.is_success() {
                eprintln!("search: HTTP {}", st);
                std::io::stdout().write_all(&body)?;
                std::process::exit(1);
            }
            std::io::stdout().write_all(&body)?;
            println!();
            Ok(())
        }
        Cmd::Open {
            entry,
            entities,
            intent,
        } => {
            if entities.is_empty() {
                return Err(anyhow!("open: --entities required (comma-separated)"));
            }
            let body_json = serde_json::to_vec(&CreateExecuteSessionBody {
                entry_id: entry,
                entities,
                principal: None,
                logical_session_id: None,
                context_intent: intent,
                ranked_capabilities: None,
            })?;
            // POST /execute returns 303 + Location; default reqwest follows redirects (GET) and we would only see 200.
            let create_client = Client::builder()
                .redirect(Policy::none())
                .build()
                .map_err(|e| anyhow!("http client (no redirect): {e}"))?;
            let url = format!("{}/execute", server.trim_end_matches('/'));
            let mut headers = HeaderMap::new();
            apply_auth_headers(&mut headers, &profile)?;
            headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            let res = create_client
                .post(url)
                .headers(headers)
                .body(body_json)
                .send()
                .await?;
            let st = res.status();
            if st != StatusCode::SEE_OTHER {
                let b = res.bytes().await?;
                eprintln!("open: expected 303, got {st}");
                std::io::stdout().write_all(&b)?;
                std::process::exit(1);
            }
            let loc = res
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow!("open: missing Location header"))?;
            let session_url = if loc.starts_with("http") {
                loc.to_string()
            } else {
                format!("{}{}", server.trim_end_matches('/'), loc)
            };
            let mut gh = HeaderMap::new();
            apply_auth_headers(&mut gh, &profile)?;
            gh.insert(ACCEPT, HeaderValue::from_static("application/json"));
            let get = client.get(&session_url).headers(gh).send().await?;
            let gst = get.status();
            let session_body = get.bytes().await?.to_vec();
            if !gst.is_success() {
                eprintln!("open: GET session failed {gst}");
                std::io::stdout().write_all(&session_body)?;
                std::process::exit(1);
            }
            // Mirror session JSON for agents / resume.
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&session_body) {
                if let (Some(ph), Some(sid)) = (
                    v.get("prompt_hash").and_then(|x| x.as_str()),
                    v.get("session").and_then(|x| x.as_str()),
                ) {
                    let p = write_mirror_meta(&server, ph, sid, "session.json", &session_body)?;
                    eprintln!("mirror: {}", p.display());
                }
            }
            std::io::stdout().write_all(&session_body)?;
            println!();
            Ok(())
        }
        Cmd::Context {
            prompt_hash,
            session,
            seeds,
            intent,
        } => {
            let mut cap_seeds = Vec::new();
            for s in seeds {
                let (eid, ent) = s
                    .split_once(':')
                    .ok_or_else(|| anyhow!("context: seeds must be entry_id:entity (got {s:?})"))?;
                cap_seeds.push(CapabilitySeed {
                    entry_id: eid.trim().to_string(),
                    entity: ent.trim().to_string(),
                });
            }
            if cap_seeds.is_empty() {
                return Err(anyhow!("context: pass at least one --seed entry_id:entity"));
            }
            let payload = serde_json::to_vec(&ExecuteSessionContextBody {
                intent,
                seeds: cap_seeds,
            })?;
            let path = format!("/execute/{prompt_hash}/{session}/context");
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::POST,
                &path,
                Some("application/json"),
                Some("application/json"),
                Some(payload),
            )
            .await?;
            if !st.is_success() {
                eprintln!("context: HTTP {}", st);
                std::io::stdout().write_all(&body)?;
                std::process::exit(1);
            }
            std::io::stdout().write_all(&body)?;
            println!();
            Ok(())
        }
        Cmd::Run {
            prompt_hash,
            session,
            mode,
            accept,
            file,
        } => {
            let body = read_program_body(file.as_ref())?;
            if body.is_empty() {
                return Err(anyhow!("run: empty program (stdin or --file)"));
            }
            let path = format!("/execute/{prompt_hash}/{session}?mode={}", mode.trim());
            let mut headers = HeaderMap::new();
            apply_auth_headers(&mut headers, &profile)?;
            headers.insert(
                ACCEPT,
                HeaderValue::from_str(accept.trim()).map_err(|e| anyhow!("accept: {e}"))?,
            );
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            let url = format!(
                "{}/{}",
                server.trim_end_matches('/'),
                path.trim_start_matches('/')
            );
            let res = client
                .post(url)
                .headers(headers)
                .body(body.clone())
                .send()
                .await?;
            let st = res.status();
            let rh = res.headers().clone();
            let out = res.bytes().await?.to_vec();
            // Mirror (no secrets): program + response + selected headers.
            let _ = write_mirror_meta(
                &server,
                &prompt_hash,
                &session,
                "latest_request.plasm",
                &body,
            );
            let _ = write_mirror_meta(&server, &prompt_hash, &session, "latest_response.bin", &out);
            let hdr_dump = format!("{st}\n{:?}", rh);
            let _ = write_mirror_meta(
                &server,
                &prompt_hash,
                &session,
                "latest_response_headers.txt",
                hdr_dump.as_bytes(),
            );
            if let Some(rid) = rh.get("x-plasm-run-id").and_then(|v| v.to_str().ok()) {
                let run_dir = session_mirror_dir(&server, &prompt_hash, &session)
                    .join("runs")
                    .join(rid);
                std::fs::create_dir_all(&run_dir)?;
                let _ = std::fs::write(run_dir.join("request.plasm"), &body);
                let _ = std::fs::write(run_dir.join("response.bin"), &out);
                let _ = std::fs::write(run_dir.join("response_headers.txt"), hdr_dump.as_bytes());
                eprintln!("mirror run: {}", run_dir.display());
            }
            if !st.is_success() {
                eprintln!("run: HTTP {}", st);
            }
            std::io::stdout().write_all(&out)?;
            if !out.ends_with(b"\n") {
                println!();
            }
            if !st.is_success() {
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Symbols {
            prompt_hash,
            session,
        } => {
            let path = format!("/execute/{prompt_hash}/{session}/symbols");
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::GET,
                &path,
                Some("application/json"),
                None,
                None,
            )
            .await?;
            if !st.is_success() {
                eprintln!("symbols: HTTP {}", st);
                std::io::stdout().write_all(&body)?;
                std::process::exit(1);
            }
            std::io::stdout().write_all(&body)?;
            println!();
            Ok(())
        }
        Cmd::Status {
            prompt_hash,
            session,
        } => {
            let path = format!("/execute/{prompt_hash}/{session}/status");
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::GET,
                &path,
                Some("application/json"),
                None,
                None,
            )
            .await?;
            if !st.is_success() {
                eprintln!("status: HTTP {}", st);
                std::io::stdout().write_all(&body)?;
                std::process::exit(1);
            }
            std::io::stdout().write_all(&body)?;
            println!();
            Ok(())
        }
        Cmd::Runs {
            prompt_hash,
            session,
        } => {
            let path = format!("/execute/{prompt_hash}/{session}/runs");
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::GET,
                &path,
                Some("application/json"),
                None,
                None,
            )
            .await?;
            if !st.is_success() {
                eprintln!("runs: HTTP {}", st);
                std::io::stdout().write_all(&body)?;
                std::process::exit(1);
            }
            std::io::stdout().write_all(&body)?;
            println!();
            Ok(())
        }
        Cmd::Artifact {
            prompt_hash,
            session,
            run_id,
        } => {
            let path = format!("/execute/{prompt_hash}/{session}/artifacts/{run_id}");
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::GET,
                &path,
                Some("application/json"),
                None,
                None,
            )
            .await?;
            if !st.is_success() {
                eprintln!("artifact: HTTP {}", st);
                std::io::stdout().write_all(&body)?;
                std::process::exit(1);
            }
            std::io::stdout().write_all(&body)?;
            println!();
            Ok(())
        }
        Cmd::MirrorPath {
            prompt_hash,
            session,
        } => {
            let p = session_mirror_dir(&server, &prompt_hash, &session);
            std::fs::create_dir_all(&p)?;
            println!("{}", p.display());
            Ok(())
        }
        Cmd::MirrorPull {
            prompt_hash,
            session,
        } => {
            let path = format!("/execute/{prompt_hash}/{session}/runs");
            let (st, _, body) = send_bytes(
                &client,
                &server,
                &profile,
                Method::GET,
                &path,
                Some("application/json"),
                None,
                None,
            )
            .await?;
            if !st.is_success() {
                return Err(anyhow!("mirror-pull: list runs failed HTTP {st}"));
            }
            let v: serde_json::Value = serde_json::from_slice(&body)?;
            let runs = v
                .get("runs")
                .and_then(|r| r.as_array())
                .ok_or_else(|| anyhow!("mirror-pull: unexpected runs JSON"))?;
            let base = session_mirror_dir(&server, &prompt_hash, &session);
            for r in runs {
                let rid = r
                    .get("run_id")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| anyhow!("mirror-pull: run without run_id"))?;
                let art_path = format!("/execute/{prompt_hash}/{session}/artifacts/{rid}");
                let (ast, _, abytes) = send_bytes(
                    &client,
                    &server,
                    &profile,
                    Method::GET,
                    &art_path,
                    Some("application/json"),
                    None,
                    None,
                )
                .await?;
                if !ast.is_success() {
                    eprintln!("mirror-pull: skip run {rid} (HTTP {ast})");
                    continue;
                }
                let run_dir = base.join("runs").join(rid);
                std::fs::create_dir_all(&run_dir)?;
                std::fs::write(run_dir.join("snapshot.json"), &abytes)?;
                println!("{}", run_dir.join("snapshot.json").display());
            }
            Ok(())
        }
    }
}
