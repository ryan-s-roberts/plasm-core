//! Load `.env` from cwd and parents without blocking on FIFOs/sockets (same hazard as schema reads).

pub fn load_from_cwd_parents() {
    let mut dir = std::env::current_dir().ok();
    for _ in 0..64 {
        let Some(d) = dir.clone() else {
            break;
        };
        let p = d.join(".env");
        if let Ok(meta) = std::fs::metadata(&p) {
            if is_safe_dotenv_file(&meta) {
                let _ = dotenvy::from_path(&p);
                return;
            }
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
}

fn is_safe_dotenv_file(meta: &std::fs::Metadata) -> bool {
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        let ft = meta.file_type();
        if ft.is_fifo() || ft.is_socket() {
            return false;
        }
    }
    true
}
