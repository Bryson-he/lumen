//! "Start with Windows": a per-user entry in Windows' own list of programs
//! to run at sign-in (the `Run` key, the same list Task Manager's "Startup
//! apps" tab shows and lets the user switch off). No service, no scheduled
//! task, no admin rights.

use std::path::Path;

use windows_registry::CURRENT_USER;

/// Where Windows keeps the per-user list of programs to run at sign-in.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// The name of Lumen's entry in that list.
pub const ENTRY: &str = "Lumen";

/// The command line for a program: its full path in quotes.
pub fn command_for(program: &Path) -> String {
    format!("\"{}\"", program.display())
}

/// The command the entry `name` under `key` runs, if there is one.
pub fn command(key: &str, name: &str) -> Option<String> {
    CURRENT_USER.open(key).ok()?.get_string(name).ok().filter(|command| !command.trim().is_empty())
}

/// Whether the entry is there and points at `program`.
pub fn is_enabled(key: &str, name: &str, program: &Path) -> bool {
    command(key, name).is_some_and(|command| command.trim().trim_matches('"').eq_ignore_ascii_case(&program.display().to_string()))
}

/// Adds the entry (pointing at `program`) or takes it away.
pub fn set_enabled(key: &str, name: &str, program: &Path, on: bool) -> Result<(), String> {
    if on {
        let run = CURRENT_USER.create(key).map_err(|error| format!("could not open Windows' start-up list: {error}"))?;
        run.set_string(name, command_for(program)).map_err(|error| format!("could not add Lumen to Windows' start-up list: {error}"))
    } else {
        let Ok(run) = CURRENT_USER.create(key) else { return Ok(()) };
        match run.remove_value(name) {
            Ok(()) => Ok(()),
            // Not there is the same as removed.
            Err(_) if run.get_string(name).is_err() => Ok(()),
            Err(error) => Err(format!("could not take Lumen out of Windows' start-up list: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_can_be_added_seen_and_taken_away() {
        // A key of the test's own, never the real start-up list.
        let root = format!(r"Software\LumenTest-{}", std::process::id());
        let key = format!(r"{root}\Run");
        let program = Path::new(r"C:\Program Files\Lumen\lumen-core.exe");
        assert!(!is_enabled(&key, ENTRY, program));
        set_enabled(&key, ENTRY, program, false).unwrap();

        set_enabled(&key, ENTRY, program, true).unwrap();
        assert_eq!(command(&key, ENTRY).as_deref(), Some(r#""C:\Program Files\Lumen\lumen-core.exe""#));
        assert!(is_enabled(&key, ENTRY, program));
        assert!(is_enabled(&key, ENTRY, Path::new(r"c:\program files\lumen\LUMEN-CORE.EXE")), "Windows paths ignore case");
        assert!(!is_enabled(&key, ENTRY, Path::new(r"D:\Elsewhere\lumen-core.exe")), "an entry for another copy is not this one's");

        set_enabled(&key, ENTRY, program, false).unwrap();
        assert!(command(&key, ENTRY).is_none());
        set_enabled(&key, ENTRY, program, false).unwrap();
        CURRENT_USER.remove_tree(&root).unwrap();
        assert!(CURRENT_USER.open(&root).is_err(), "nothing is left in the registry");
    }
}
