//! yate, yet another text editor: a modeless terminal editor with a file
//! tree, fuzzy file search, live grep, go to definition and JavaScript plugins.

use std::process::ExitCode;

use yate::app;
use yate::{HELP, update};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["update"] {
        return match update::execute() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("yate: {err}");
                ExitCode::FAILURE
            }
        };
    }
    if args == ["init"] {
        return init();
    }
    if args == ["skill"] {
        print!("{}", yate::skill::instructions());
        return ExitCode::SUCCESS;
    }
    if args == ["skill", "--init"] {
        return skill_init();
    }
    let mut path = None;
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-v" | "--version" => {
                println!("yate {}", update::VERSION);
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("yate: unknown option {other} (see yate --help)");
                return ExitCode::FAILURE;
            }
            other if path.is_none() => path = Some(other.to_string()),
            _ => {
                eprintln!("yate: only one path can be opened (see yate --help)");
                return ExitCode::FAILURE;
            }
        }
    }
    match app::run(path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("yate: {err}");
            ExitCode::FAILURE
        }
    }
}

fn init() -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    match yate::settings::init(&cwd) {
        Ok(path) => {
            println!(
                "yate: created {}; remove the // in front of a setting to change it",
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("yate: {err}");
            ExitCode::FAILURE
        }
    }
}

fn skill_init() -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match yate::skill::init(&cwd, home.as_deref()) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("yate: {err}");
            ExitCode::FAILURE
        }
    }
}
