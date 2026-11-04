//! Starting Orange at sign-in through a Windows scheduled task.
//!
//! The normal "Run" entry is launched by Windows only after it has finished loading its other startup
//! programs, which on a busy laptop can be a minute or more. A scheduled task with an "at log on" trigger
//! starts straight away. The task runs as the current user with normal rights, so no administrator
//! account is needed. The ordinary startup entry stays as a fallback; Orange only ever runs one copy.

/// The name of the scheduled task.
pub const TASK_NAME: &str = "Orange startup";

/// Quote text as a PowerShell single-quoted string.
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The PowerShell that creates (or replaces) the task for the program at `exe`.
pub fn register_script(exe: &str) -> String {
    format!(
        "$ErrorActionPreference='Stop'; \
         $a=New-ScheduledTaskAction -Execute {exe} -Argument '--hidden'; \
         $t=New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME; \
         $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero); \
         $p=New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited; \
         Register-ScheduledTask -TaskName {name} -Action $a -Trigger $t -Settings $s -Principal $p -Force | Out-Null",
        exe = ps_quote(exe),
        name = ps_quote(TASK_NAME),
    )
}

/// The PowerShell that removes the task, quietly, when it is not there.
pub fn unregister_script() -> String {
    format!("Unregister-ScheduledTask -TaskName {} -Confirm:$false -ErrorAction SilentlyContinue", ps_quote(TASK_NAME))
}

#[cfg(windows)]
fn run_powershell(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Make sure the task exists and points at this copy of Orange.
#[cfg(windows)]
pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    run_powershell(&register_script(&exe.to_string_lossy()))
}

/// Remove the task.
#[cfg(windows)]
pub fn disable() -> Result<(), String> {
    run_powershell(&unregister_script())
}

#[cfg(not(windows))]
pub fn enable() -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn disable() -> Result<(), String> {
    Ok(())
}

/// Bring the task in line with the "Start Orange when I sign in" setting, on a background thread so
/// start-up and the Settings window never wait for PowerShell.
pub fn sync(enabled: bool) {
    let _ = std::thread::Builder::new().name("orange-startup-task".into()).spawn(move || {
        // let the desktop settle first when this runs during sign-in
        if enabled {
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        let r = if enabled { enable() } else { disable() };
        match r {
            Ok(()) => log::info!("startup task {}", if enabled { "is set" } else { "is removed" }),
            Err(e) => log::warn!("startup task: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_apostrophes_in_paths() {
        assert_eq!(ps_quote(r"C:\Users\O'Brien\Orange.exe"), r"'C:\Users\O''Brien\Orange.exe'");
    }

    #[test]
    fn register_script_names_task_program_and_hidden_flag() {
        let s = register_script(r"C:\Users\ACER\AppData\Local\Orange\Orange.exe");
        assert!(s.contains("-Execute 'C:\\Users\\ACER\\AppData\\Local\\Orange\\Orange.exe'"));
        assert!(s.contains("-Argument '--hidden'"));
        assert!(s.contains("-AtLogOn"));
        assert!(s.contains("-RunLevel Limited"));
        assert!(s.contains("-AllowStartIfOnBatteries"));
        assert!(s.contains("-TaskName 'Orange startup'"));
    }

    #[test]
    fn unregister_script_is_quiet() {
        let s = unregister_script();
        assert!(s.contains("-TaskName 'Orange startup'"));
        assert!(s.contains("SilentlyContinue"));
    }
}
