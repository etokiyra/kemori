use std::process::ExitCode;

use kemori::app::App;

fn main() -> ExitCode {
    // Kemori is usable both as a library and as a CLI. The CLI accepts an optional
    // path: `kemori`, `kemori .`, or `kemori path/to/file.rs`.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = args.into_iter().find(|a| !a.starts_with('-'));

    match App::start(target) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("kemori: {err}");
            ExitCode::FAILURE
        }
    }
}
