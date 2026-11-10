//! A small append-only log in `~/.local/state/yate/yate.log`, for errors that
//! should not interrupt editing (plugin failures, query errors).

use std::io::Write;

pub fn write(message: &str) {
    write_to(&crate::config::state_dir(), message);
}

pub fn write_to(dir: &std::path::Path, message: &str) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("yate.log"))
    else {
        return;
    };
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(file, "[{seconds}] {message}");
}
