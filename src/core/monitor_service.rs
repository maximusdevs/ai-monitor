//! `ai-monitor monitor --install-service` / `--uninstall-service`: start the
//! renewal monitor at login with the OS's own per-user mechanism, so renewal
//! notifications work without the user keeping a terminal open.
//!
//! - Linux: a systemd user unit in `$XDG_CONFIG_HOME/systemd/user/`.
//! - macOS: a LaunchAgent in `~/Library/LaunchAgents/`.
//! - Windows: a `HKCU\…\Run` value. A logon scheduled task would need an
//!   elevated prompt; the Run key does not.
//!
//! Every file or command line is built by a pure function below so the tests
//! never touch the real service manager, registry or home directory.

use std::path::{Path, PathBuf};

/// systemd unit name and LaunchAgent / Run-value stem.
pub const SERVICE_NAME: &str = "ai-monitor-daemon";
/// LaunchAgent label, in the same namespace as the menu bar app's agent.
pub const LAUNCHD_LABEL: &str = "com.maximusdevs.ai-monitor-daemon";
/// Value name under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub const WINDOWS_RUN_VALUE: &str = "AIMonitorDaemon";

/// Quote one argument for a systemd `ExecStart=` line. Inside double quotes
/// systemd still expands `%` specifiers and `$` variables, so both are doubled.
fn systemd_quote(arg: &str) -> String {
    let escaped = arg
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The systemd user unit. `default.target` rather than
/// `graphical-session.target`: many Wayland sessions never activate the
/// latter, and `notify-send` reaches the session bus that the user manager
/// already exports either way.
pub fn systemd_unit(exe: &Path, interval: u64) -> String {
    let exe = systemd_quote(&exe.to_string_lossy());
    format!(
        "[Unit]\n\
         Description=ai-monitor quota renewal monitor\n\
         \n\
         [Service]\n\
         ExecStart={exe} monitor --interval {interval}\n\
         Restart=on-failure\n\
         RestartSec=30\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

/// The LaunchAgent plist. `KeepAlive` restarts it after a crash; a clean exit
/// (the loop never exits cleanly) would also be restarted, which is the intent.
pub fn launchd_plist(exe: &Path, interval: u64, log: &Path) -> String {
    let exe = xml_escape(&exe.to_string_lossy());
    let log = xml_escape(&log.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>monitor</string>
        <string>--interval</string>
        <string>{interval}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#
    )
}

/// The command stored in the Run key. `ai-monitor.exe` is a console program,
/// so launching it directly at logon would leave a console window open; a
/// hidden PowerShell host gives it an invisible console instead.
pub fn windows_run_command(exe: &Path, interval: u64) -> String {
    let exe = exe.to_string_lossy().replace('\'', "''");
    format!(
        "powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -Command \
         \"& '{exe}' monitor --interval {interval}\""
    )
}

/// Install and start the service. Returns the process exit code.
pub fn install(interval: u64) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            eprintln!("ai-monitor monitor: cannot locate this executable: {error}");
            return 1;
        }
    };
    match platform::install(&exe, interval) {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(message) => {
            eprintln!("ai-monitor monitor: {message}");
            1
        }
    }
}

/// Stop and remove the service. Returns the process exit code.
pub fn uninstall() -> i32 {
    match platform::uninstall() {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(message) => {
            eprintln!("ai-monitor monitor: {message}");
            1
        }
    }
}

/// Where a running Windows monitor records its pid, so `--uninstall-service`
/// stops that instance without killing every `ai-monitor.exe`.
pub fn pid_path() -> Option<PathBuf> {
    crate::cache::xdg_cache_dir()
        .ok()
        .map(|dir| dir.join("ai-monitor").join("monitor.pid"))
}

#[cfg(unix)]
fn write_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", display(parent)))?;
    }
    std::fs::write(path, contents)
        .map_err(|error| format!("cannot write {}: {error}", display(path)))
}

#[cfg(unix)]
fn display(path: &Path) -> String {
    crate::display::sanitize_untrusted_path(path)
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use super::*;
    use std::process::Command;

    fn unit_path() -> Result<PathBuf, String> {
        let base = directories::BaseDirs::new().ok_or("could not resolve the home directory")?;
        Ok(base
            .config_dir()
            .join("systemd/user")
            .join(format!("{SERVICE_NAME}.service")))
    }

    fn systemctl(args: &[&str]) -> Result<(), String> {
        match Command::new("systemctl").arg("--user").args(args).status() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!(
                "`systemctl --user {}` exited with {status}",
                args.join(" ")
            )),
            Err(error) => Err(format!("cannot run systemctl: {error}")),
        }
    }

    pub fn install(exe: &Path, interval: u64) -> Result<String, String> {
        let path = unit_path()?;
        write_file(&path, &systemd_unit(exe, interval))?;
        let unit = format!("{SERVICE_NAME}.service");
        if let Err(error) =
            systemctl(&["daemon-reload"]).and_then(|()| systemctl(&["enable", "--now", &unit]))
        {
            return Err(format!(
                "wrote {} but could not start it ({error}). Without systemd, add \
                 `{} monitor` to your session's autostart instead.",
                display(&path),
                display(exe)
            ));
        }
        Ok(format!(
            "✓ {unit} enabled and started.\n  Logs: journalctl --user -u {SERVICE_NAME} -f"
        ))
    }

    pub fn uninstall() -> Result<String, String> {
        let path = unit_path()?;
        let unit = format!("{SERVICE_NAME}.service");
        let _ = systemctl(&["disable", "--now", &unit]);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(format!("{unit} was not installed."));
            }
            Err(error) => return Err(format!("cannot remove {}: {error}", display(&path))),
        }
        let _ = systemctl(&["daemon-reload"]);
        Ok(format!("✓ {unit} stopped and removed."))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::process::Command;

    fn home() -> Result<PathBuf, String> {
        crate::cache::home_dir().map_err(|error| error.to_string())
    }

    fn plist_path() -> Result<PathBuf, String> {
        Ok(home()?
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist")))
    }

    pub fn install(exe: &Path, interval: u64) -> Result<String, String> {
        let path = plist_path()?;
        let log = home()?
            .join("Library/Logs")
            .join(format!("{SERVICE_NAME}.log"));
        write_file(&path, &launchd_plist(exe, interval, &log))?;
        let _ = Command::new("launchctl").arg("unload").arg(&path).output();
        match Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&path)
            .status()
        {
            Ok(status) if status.success() => Ok(format!(
                "✓ {LAUNCHD_LABEL} loaded (starts at login).\n  Logs: {}",
                display(&log)
            )),
            Ok(status) => Err(format!("`launchctl load` exited with {status}")),
            Err(error) => Err(format!("cannot run launchctl: {error}")),
        }
    }

    pub fn uninstall() -> Result<String, String> {
        let path = plist_path()?;
        if !path.exists() {
            return Ok(format!("{LAUNCHD_LABEL} was not installed."));
        }
        let _ = Command::new("launchctl")
            .args(["unload", "-w"])
            .arg(&path)
            .output();
        std::fs::remove_file(&path)
            .map_err(|error| format!("cannot remove {}: {error}", display(&path)))?;
        Ok(format!("✓ {LAUNCHD_LABEL} unloaded and removed."))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    fn quiet(program: &str) -> Command {
        let mut command = Command::new(program);
        command.creation_flags(crate::process::CREATE_NO_WINDOW);
        command
    }

    /// Stop the instance recorded in the pid file, if it is still ours.
    fn stop_running() {
        let Some(path) = pid_path() else { return };
        if let Ok(pid) = std::fs::read_to_string(&path) {
            let pid = pid.trim();
            if pid.chars().all(|c| c.is_ascii_digit()) && !pid.is_empty() {
                let _ = quiet("taskkill")
                    .args(["/PID", pid, "/FI", "IMAGENAME eq ai-monitor.exe", "/F"])
                    .output();
            }
        }
        let _ = std::fs::remove_file(path);
    }

    pub fn install(exe: &Path, interval: u64) -> Result<String, String> {
        let command = windows_run_command(exe, interval);
        let status = quiet("reg")
            .args([
                "add",
                RUN_KEY,
                "/v",
                WINDOWS_RUN_VALUE,
                "/t",
                "REG_SZ",
                "/d",
                &command,
                "/f",
            ])
            .output()
            .map_err(|error| format!("cannot run reg.exe: {error}"))?;
        if !status.status.success() {
            return Err(format!("`reg add` exited with {}", status.status));
        }
        // Start one now rather than at the next sign-in, replacing any earlier one.
        stop_running();
        quiet(&exe.to_string_lossy())
            .args(["monitor", "--interval", &interval.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("registered, but could not start the monitor: {error}"))?;
        Ok("✓ Monitor registered to start at sign-in and started now.".into())
    }

    pub fn uninstall() -> Result<String, String> {
        stop_running();
        let output = quiet("reg")
            .args(["delete", RUN_KEY, "/v", WINDOWS_RUN_VALUE, "/f"])
            .output()
            .map_err(|error| format!("cannot run reg.exe: {error}"))?;
        if output.status.success() {
            Ok("✓ Monitor stopped and removed from sign-in.".into())
        } else {
            Ok("Monitor was not registered.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_unit_quotes_the_executable_and_escapes_specifiers() {
        let unit = systemd_unit(Path::new("/home/a b/100%/$bin/ai-monitor"), 45);
        assert!(
            unit.contains("ExecStart=\"/home/a b/100%%/$$bin/ai-monitor\" monitor --interval 45\n")
        );
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("Restart=on-failure"));
    }

    #[test]
    fn launchd_plist_escapes_paths_and_passes_the_interval() {
        let plist = launchd_plist(
            Path::new("/Apps/A&B/ai-monitor"),
            30,
            Path::new("/Users/x/Library/Logs/m.log"),
        );
        assert!(plist.contains("<string>/Apps/A&amp;B/ai-monitor</string>"));
        assert!(plist.contains("<string>monitor</string>"));
        assert!(plist.contains("<string>30</string>"));
        assert!(plist.contains(&format!("<string>{LAUNCHD_LABEL}</string>")));
        assert!(plist.contains("<key>KeepAlive</key>"));
    }

    #[test]
    fn windows_run_command_is_hidden_and_escapes_single_quotes() {
        let command = windows_run_command(Path::new(r"C:\Users\O'Neil\ai-monitor.exe"), 60);
        assert!(command.starts_with("powershell.exe "));
        assert!(command.contains("-WindowStyle Hidden"));
        assert!(
            command.ends_with(r#""& 'C:\Users\O''Neil\ai-monitor.exe' monitor --interval 60""#)
        );
    }
}
