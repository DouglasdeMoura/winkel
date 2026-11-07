use std::path::PathBuf;

// Where the catalogues live: WINKEL_UI first, so a checkout always wins for
// its own runs; then beside the binary; then a capped walk up the tree, which
// finds both the cargo checkout (`target/release/winkel` → `ui/` at the repo
// root) and a prefixed install (`/usr/local/bin/winkel` →
// `/usr/local/share/winkel/ui`); then the system path pacman owns. The marker
// is en.json — the QML shell is gone.
pub fn ui_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("WINKEL_UI") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent().map(PathBuf::from)?;
    if dir.join("ui").join("i18n").join("en.json").is_file() {
        return Some(dir.join("ui"));
    }
    for _ in 0..4 {
        let repo = dir.join("ui");
        if repo.join("i18n").join("en.json").is_file() {
            return Some(repo);
        }
        let installed = dir.join("share/winkel/ui");
        if installed.join("i18n").join("en.json").is_file() {
            return Some(installed);
        }
        if !dir.pop() {
            break;
        }
    }
    let system = PathBuf::from("/usr/share/winkel/ui");
    if system.join("i18n").join("en.json").is_file() {
        return Some(system);
    }
    None
}

pub fn has_display() -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()))
}

// Whether a Winkel backend already holds the single-instance lock.
pub fn backend_running() -> bool {
    crate::backend::run::instance_is_running()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_display_is_wayland_or_x() {
        // The assertion runs on the machine building Winkel; headless CI keeps
        // both unset, a desktop always has one. Either way the answer is honest.
        let has = super::has_display();
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        let x = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());
        assert_eq!(has, wayland || x);
    }
}
