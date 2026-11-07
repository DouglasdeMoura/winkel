mod backend;
mod gui;
mod json;
mod paths;

#[cfg(test)]
mod i18n_check;

use std::process::exit;

fn usage(message: &str) -> ! {
    eprintln!("winkel: {}", message);
    eprintln!("usage: winkel [--backend]");
    eprintln!("       winkel --version");
    exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() == 2 && args[1] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }
    if args.iter().any(|a| a == "--version") {
        usage("--version takes nothing");
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage("help:");
    }

    // winkel --backend is the process the window spawns; the protocol lives in
    // docs/protocol.md and its one interpreter lives in backend/proto.rs.
    if args.len() == 2 && args[1] == "--backend" {
        exit(backend::run::run());
    }
    if args.iter().any(|a| a == "--backend") {
        usage("--backend takes nothing");
    }

    if args.len() > 1 {
        usage(&format!("unknown argument {}", args[1]));
    }

    exit(gui::launch());
}
