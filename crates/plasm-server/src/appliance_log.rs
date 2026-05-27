//! Typed `tracing` capture for the Ratatui **Logs** tab; optional fmt duplicate for diagnostics.
//!
//! TUI mode: [`plasm_otel::TuiLogCallback`] → [`ApplianceLogEntry`]. The fmt layer writes to
//! [`std::io::sink()`] or [`ApplianceDiagFileMakeWriter`] when `PLASM_APPLIANCE_DIAG_LOG` is set.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crossbeam_channel::Sender;
use plasm_otel::{TuiLogCallback, TuiLogRecord};
use tracing::Level;
use tracing_subscriber::fmt::MakeWriter;

pub const APPLIANCE_LOG_CHANNEL_CAP: usize = 8192;
pub const APPLIANCE_LOG_TAB_MAX_LINES: usize = 2000;

/// One log line in the control-station Logs tab (from the TUI capture layer, not fmt parsing).
#[derive(Clone, Debug)]
pub struct ApplianceLogEntry {
    pub timestamp: SystemTime,
    pub level: Level,
    pub target: String,
    pub message: String,
}

impl From<TuiLogRecord> for ApplianceLogEntry {
    fn from(rec: TuiLogRecord) -> Self {
        Self {
            timestamp: rec.timestamp,
            level: rec.level,
            target: rec.target,
            message: rec.message,
        }
    }
}

/// Fmt layer target for appliance TUI mode: diag file or discard (human-readable logs use [`TuiLogCallback`]).
#[derive(Clone)]
pub enum ApplianceFmtMakeWriter {
    Diag(ApplianceDiagFileMakeWriter),
    Sink,
}

impl<'a> MakeWriter<'a> for ApplianceFmtMakeWriter {
    type Writer = ApplianceFmtWriter;
    fn make_writer(&'a self) -> Self::Writer {
        match self {
            Self::Diag(d) => ApplianceFmtWriter::Diag(d.make_writer()),
            Self::Sink => ApplianceFmtWriter::Sink(std::io::sink()),
        }
    }
}

pub enum ApplianceFmtWriter {
    Diag(ApplianceDiagFileWriter),
    Sink(std::io::Sink),
}

impl Write for ApplianceFmtWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Diag(w) => w.write(buf),
            Self::Sink(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Diag(w) => w.flush(),
            Self::Sink(w) => w.flush(),
        }
    }
}

/// Resolve fmt writer from `PLASM_APPLIANCE_DIAG_LOG` when set.
pub fn appliance_fmt_make_writer(
    diag_path: Option<&Path>,
) -> Result<ApplianceFmtMakeWriter, io::Error> {
    match diag_path {
        Some(p) if !p.as_os_str().is_empty() => {
            ApplianceDiagFileMakeWriter::open(p).map(ApplianceFmtMakeWriter::Diag)
        }
        _ => Ok(ApplianceFmtMakeWriter::Sink),
    }
}

/// Build a [`TuiLogCallback`] that forwards into the bounded UI channel.
pub fn appliance_tui_callback(tx: Sender<ApplianceLogEntry>) -> TuiLogCallback {
    Arc::new(move |rec| {
        let _ = tx.try_send(ApplianceLogEntry::from(rec));
    })
}

/// Append-only fmt sink for `PLASM_APPLIANCE_DIAG_LOG` (PTY e2e diagnostics).
#[derive(Clone)]
pub struct ApplianceDiagFileMakeWriter {
    file: Arc<Mutex<std::fs::File>>,
}

impl ApplianceDiagFileMakeWriter {
    pub fn open(path: &Path) -> io::Result<Self> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(f)),
        })
    }
}

impl<'a> MakeWriter<'a> for ApplianceDiagFileMakeWriter {
    type Writer = ApplianceDiagFileWriter;
    fn make_writer(&'a self) -> Self::Writer {
        ApplianceDiagFileWriter {
            buf: Vec::new(),
            file: Arc::clone(&self.file),
        }
    }
}

pub struct ApplianceDiagFileWriter {
    buf: Vec<u8>,
    file: Arc<Mutex<std::fs::File>>,
}

impl ApplianceDiagFileWriter {
    fn emit_line(&mut self, line: String) {
        let line =
            String::from_utf8_lossy(&strip_ansi_escapes::strip(line.as_bytes())).into_owned();
        if let Ok(mut g) = self.file.lock() {
            let _ = writeln!(g, "{line}");
            let _ = g.flush();
        }
    }
}

impl Write for ApplianceDiagFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buf.drain(..=pos).collect();
            let end = line_bytes.len().saturating_sub(1);
            let line = String::from_utf8_lossy(&line_bytes[..end]).into_owned();
            self.emit_line(line);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            let line = String::from_utf8_lossy(&self.buf).into_owned();
            self.buf.clear();
            self.emit_line(line);
        }
        Ok(())
    }
}

/// Push plain multi-line help text into the Logs tab (not via `tracing`).
pub fn push_block(tx: &Sender<ApplianceLogEntry>, text: &str) {
    if text.is_empty() {
        return;
    }
    let now = SystemTime::now();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let _ = tx.try_send(ApplianceLogEntry {
            timestamp: now,
            level: Level::INFO,
            target: "plasm_appliance".into(),
            message: line.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    #[test]
    fn appliance_tui_callback_forwards_entries() {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let cb = appliance_tui_callback(tx);

        let _guard = tracing_subscriber::registry()
            .with(plasm_otel::tui_capture_layer(Some(cb)))
            .with(EnvFilter::new("info"))
            .set_default();

        tracing::error!(target: "appliance_test", "boot failed");

        let entry = rx.try_recv().expect("one log entry");
        assert_eq!(entry.level, Level::ERROR);
        assert_eq!(entry.target, "appliance_test");
        assert_eq!(entry.message, "boot failed");
    }
}
