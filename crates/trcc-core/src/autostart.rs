//! "Start with Windows" through the current user's Run key. No admin rights needed.

use std::path::Path;

use windows_registry::CURRENT_USER;

use crate::Error;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "TRCC-LCD";

fn command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

#[allow(clippy::needless_pass_by_value)]
fn registry(e: impl std::fmt::Display) -> Error {
    Error::Registry(e.to_string())
}

/// Whether `exe` is set to start when the user signs in.
#[must_use]
pub fn is_enabled(exe: &Path) -> bool {
    is_enabled_as(VALUE, exe)
}

pub fn set(exe: &Path, enabled: bool) -> Result<(), Error> {
    set_as(VALUE, exe, enabled)
}

fn is_enabled_as(name: &str, exe: &Path) -> bool {
    CURRENT_USER
        .open(RUN_KEY)
        .and_then(|k| k.get_string(name))
        .is_ok_and(|v| v.eq_ignore_ascii_case(&command(exe)))
}

fn set_as(name: &str, exe: &Path, enabled: bool) -> Result<(), Error> {
    let key = CURRENT_USER.create(RUN_KEY).map_err(registry)?;
    if enabled {
        key.set_string(name, command(exe)).map_err(registry)?;
    } else if key.get_string(name).is_ok() {
        key.remove_value(name).map_err(registry)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_run_entry() {
        let name = format!("TRCC-LCD-test-{}", std::process::id());
        let exe = Path::new(r"C:\test\trcc-lcd-tray.exe");
        set_as(&name, exe, true).unwrap();
        assert!(is_enabled_as(&name, exe));
        assert!(!is_enabled_as(&name, Path::new(r"C:\other.exe")));
        set_as(&name, exe, false).unwrap();
        assert!(!is_enabled_as(&name, exe));
        set_as(&name, exe, false).unwrap();
    }
}
