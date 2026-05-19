//! Forward `tracing` fmt output into the Ratatui **Logs** tab (never stderr during alternate-screen UI).
//!
//! Optional append-only file duplicate: see [`ApplianceLogMakeWriter::with_diag_file`] and
//! `PLASM_APPLIANCE_DIAG_LOG` in `docs/appliance-surface-inventory.md`.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use tracing_subscriber::fmt::MakeWriter;

pub const APPLIANCE_LOG_CHANNEL_CAP: usize = 8192;
pub const APPLIANCE_LOG_TAB_MAX_LINES: usize = 2000;

#[derive(Clone)]
pub struct ApplianceLogMakeWriter {
    tx: Sender<String>,
    diag: Option<Arc<Mutex<std::fs::File>>>,
}

impl ApplianceLogMakeWriter {
    pub fn new(tx: Sender<String>) -> Self {
        Self { tx, diag: None }
    }

    /// Duplicate each completed fmt line to an append-only file (e.g. PTY e2e diagnostics).
    pub fn with_diag_file(tx: Sender<String>, file: std::fs::File) -> Self {
        Self {
            tx,
            diag: Some(Arc::new(Mutex::new(file))),
        }
    }
}

impl<'a> MakeWriter<'a> for ApplianceLogMakeWriter {
    type Writer = ApplianceLogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        ApplianceLogWriter {
            buf: Vec::new(),
            tx: self.tx.clone(),
            diag: self.diag.clone(),
        }
    }
}

pub struct ApplianceLogWriter {
    buf: Vec<u8>,
    tx: Sender<String>,
    diag: Option<Arc<Mutex<std::fs::File>>>,
}

impl ApplianceLogWriter {
    fn emit_line(&mut self, line: String) {
        let line =
            String::from_utf8_lossy(&strip_ansi_escapes::strip(line.as_bytes())).into_owned();
        let _ = self.tx.try_send(line.clone());
        if let Some(f) = &self.diag {
            if let Ok(mut g) = f.lock() {
                let _ = writeln!(g, "{line}");
                let _ = g.flush();
            }
        }
    }
}

impl Write for ApplianceLogWriter {
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

/// Headless `--no-tui` diagnostics: append-only `PLASM_APPLIANCE_DIAG_LOG` without a TUI log channel.
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

pub fn push_block(tx: &Sender<String>, text: &str) {
    if text.is_empty() {
        return;
    }
    for line in text.lines() {
        let clean =
            String::from_utf8_lossy(&strip_ansi_escapes::strip(line.as_bytes())).into_owned();
        let _ = tx.try_send(clean);
    }
}
