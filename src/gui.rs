use crate::paths;
use std::path::PathBuf;

pub fn launch() -> i32 {
    // One Winkel at a time: a second metronome half a beat out from the first
    // makes ticks fuse and vanish. Answer before a window is spawned so the
    // second launch is a polite no-op, not a broken shell.
    if paths::backend_running() {
        eprintln!("winkel: Winkel is already running");
        return 0;
    }
    if !paths::has_display() {
        eprintln!("winkel: there is no graphical session to open a window in");
        return 2;
    }
    if paths::ui_dir().is_none() {
        eprintln!(
            "winkel: the catalogues are missing, set WINKEL_UI or install /usr/share/winkel/ui"
        );
        return 2;
    }

    // The backend binary the window spawns is this one, unless the caller
    // pinned it — WINKEL_BIN is the same seam the QML shell used.
    let backend_bin = match std::env::var_os("WINKEL_BIN") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("winkel: could not find this binary to start the backend");
                return 1;
            }
        },
    };
    // So a child that re-reads the env (and the UI's own lookups) sees the
    // pinned path, not a bare "winkel" that might miss the checkout build.
    if std::env::var_os("WINKEL_BIN").is_none_or(|v| v.is_empty()) {
        // Safety: set_var is process-wide; we are single-threaded here before
        // the gpui runtime starts.
        unsafe { std::env::set_var("WINKEL_BIN", &backend_bin) };
    }

    winkel_ui::run(backend_bin.to_string_lossy().into_owned());
    0
}
