//! Development tasks, run through the cargo aliases in `.cargo/config.toml`:
//!
//! - `cargo verify`: formatting, lints and tests (run before committing)
//! - `cargo update-snapshots`: accepts the current screens of the end-to-end
//!   tests as their snapshots
//! - `cargo dist`: a static release binary in `dist/yate` (musl on Linux)

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn is_package_variable_of_xtask(name: &str) -> bool {
    name.starts_with("CARGO_PKG_")
        || name.starts_with("CARGO_MANIFEST_")
        || matches!(
            name,
            "CARGO_CRATE_NAME" | "CARGO_BIN_NAME" | "CARGO_PRIMARY_PACKAGE"
        )
}

fn cargo(args: &[&str], env: &[(&str, &str)]) -> bool {
    println!("> cargo {}", args.join(" "));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo);
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(is_package_variable_of_xtask) {
            command.env_remove(name);
        }
    }
    command
        .args(args)
        .envs(env.iter().copied())
        .current_dir(workspace_root())
        .status()
        .is_ok_and(|status| status.success())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace")
        .to_path_buf()
}

fn musl_target() -> Option<&'static str> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let target = if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-musl"
    } else {
        "x86_64-unknown-linux-musl"
    };
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(target))
        .unwrap_or(false);
    installed.then_some(target)
}

fn dist() -> bool {
    let root = workspace_root();
    let target = musl_target();
    let mut args = vec!["build", "--release", "--locked", "--package", "yate"];
    if let Some(target) = target {
        args.extend(["--target", target]);
    } else if cfg!(target_os = "linux") {
        println!(
            "musl target not installed, building a glibc binary (rustup target add <arch>-unknown-linux-musl for a static one)"
        );
    }
    if !cargo(&args, &[]) {
        return false;
    }
    let built = match target {
        Some(target) => root.join("target").join(target).join("release/yate"),
        None => root.join("target/release/yate"),
    };
    let dist = root.join("dist");
    if std::fs::create_dir_all(&dist).is_err() || std::fs::copy(&built, dist.join("yate")).is_err()
    {
        eprintln!("could not copy {} to dist/", built.display());
        return false;
    }
    println!("dist/yate ready");
    true
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let ok = match task.as_str() {
        "verify" => {
            cargo(&["fmt", "--all", "--check"], &[])
                && cargo(
                    &[
                        "clippy",
                        "--workspace",
                        "--all-targets",
                        "--",
                        "-D",
                        "warnings",
                    ],
                    &[],
                )
                && cargo(&["test", "--workspace"], &[])
        }
        "update-snapshots" => cargo(&["test", "--test", "e2e"], &[("UPDATE_SNAPSHOTS", "1")]),
        "dist" => dist(),
        _ => {
            eprintln!("Usage: cargo xtask <verify|update-snapshots|dist>");
            false
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
