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
