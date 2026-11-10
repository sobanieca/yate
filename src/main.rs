//! yate, yet another text editor: a modeless terminal editor with a file
//! tree, fuzzy file search, live grep, git review and JavaScript plugins.

use std::process::ExitCode;

use yate::app::{self, Launch};
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
    if args.first().is_some_and(|arg| arg == "diff") {
        let [_, left, right] = args.as_slice() else {
            eprintln!("yate: diff takes two files or two folders (see yate --help)");
            return ExitCode::FAILURE;
        };
        return launch(Launch::Diff(left.clone(), right.clone()));
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
    launch(Launch::Open(path))
}

fn launch(launch: Launch) -> ExitCode {
    match app::run(launch) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("yate: {err}");
            ExitCode::FAILURE
        }
    }
}
