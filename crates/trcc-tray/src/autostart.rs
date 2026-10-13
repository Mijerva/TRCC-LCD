//! "Start with Windows" through the current user's Run key. No admin rights needed.

use windows_registry::CURRENT_USER;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "TRCC-LCD";

fn command() -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;
    Ok(format!("\"{}\"", exe.display()))
}

pub fn is_enabled() -> bool {
    is_enabled_as(VALUE)
}

pub fn set(enabled: bool) -> anyhow::Result<()> {
    set_as(VALUE, enabled)
}

fn is_enabled_as(name: &str) -> bool {
    let Ok(expected) = command() else {
        return false;
    };
    CURRENT_USER
        .open(RUN_KEY)
        .and_then(|k| k.get_string(name))
        .is_ok_and(|v| v.eq_ignore_ascii_case(&expected))
}

fn set_as(name: &str, enabled: bool) -> anyhow::Result<()> {
    let key = CURRENT_USER.create(RUN_KEY)?;
    if enabled {
        key.set_string(name, command()?)?;
    } else if key.get_string(name).is_ok() {
        key.remove_value(name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_run_entry() {
        let name = format!("TRCC-LCD-test-{}", std::process::id());
        set_as(&name, true).unwrap();
        assert!(is_enabled_as(&name));
        set_as(&name, false).unwrap();
        assert!(!is_enabled_as(&name));
        set_as(&name, false).unwrap();
    }
}
