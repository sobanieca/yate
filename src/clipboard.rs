//! Copy and paste. Inside tmux, copied text goes to a tmux paste buffer
//! (`tmux load-buffer -w`, which also forwards it to the terminal clipboard
//! with `set-clipboard on`), and paste reads the newest tmux buffer. Outside
//! tmux, copies are sent to the terminal as OSC 52.

use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Default)]
pub struct Clipboard {
    /// Keeps copies inside yate only (tests): no tmux, no OSC 52.
    in_memory: bool,
    text: String,
    /// The text was a whole line copied without a selection; pasting it
    /// inserts it above the cursor line, like VS Code.
    linewise: bool,
}

pub enum CopyOutcome {
    Memory,
    Tmux,
    /// The OSC 52 sequence the terminal must receive.
    Osc52(Vec<u8>),
}

fn in_tmux() -> bool {
    std::env::var_os("TMUX").is_some_and(|value| !value.is_empty())
}

impl Clipboard {
    pub fn in_memory() -> Clipboard {
        Clipboard {
            in_memory: true,
            ..Clipboard::default()
        }
    }

    pub fn copy(&mut self, text: &str, linewise: bool) -> CopyOutcome {
        self.text = text.to_string();
        self.linewise = linewise;
        if self.in_memory {
            return CopyOutcome::Memory;
        }
        if in_tmux() && (tmux_load(text, true) || tmux_load(text, false)) {
            return CopyOutcome::Tmux;
        }
        CopyOutcome::Osc52(osc52(text))
    }

    /// The text to paste and whether it is a whole copied line.
    pub fn paste(&self) -> (String, bool) {
        if !self.in_memory
            && in_tmux()
            && let Some(text) = tmux_buffer()
        {
            let linewise = self.linewise && text == self.text;
            return (text, linewise);
        }
        (self.text.clone(), self.linewise)
    }
}

fn tmux_load(text: &str, forward_to_clipboard: bool) -> bool {
    let mut args = vec!["load-buffer"];
    if forward_to_clipboard {
        args.push("-w");
    }
    args.push("-");
    let Ok(mut child) = Command::new("tmux")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().is_ok_and(|status| status.success())
}

fn tmux_buffer() -> Option<String> {
    let output = Command::new("tmux")
        .args(["save-buffer", "-"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn osc52(text: &str) -> Vec<u8> {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes())).into_bytes()
}

pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("zażółć".as_bytes()), "emHFvMOzxYLEhw==");
    }
}
