//! Starting the app when the user logs in: an entry the system keeps, not
//! a setting. On Linux an XDG autostart file, on macOS a LaunchAgent, on
//! Windows a value under `HKCU\...\Run`. The entry is what says whether
//! it's on, so turning it off in the system's own settings shows here too.
//!
//! The entry launches this executable with `--background` (into the tray)
//! and the `--data-dir` it was given. It's named after the data dir, like
//! the single-instance socket, so an instance on another data dir (a test
//! run, a second profile) has its own entry and never touches this one's.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::APP_ID;

/// Starts the app at login, or stops doing so. Calls touch files or the
/// registry, so the shell changes it off the UI thread.
pub trait LoginItem: Send + Sync + 'static {
    /// Whether the system starts the app at login.
    fn is_enabled(&self) -> bool;
    /// Add or remove the entry. Adding it again rewrites the command, for
    /// an app that moved.
    fn set_enabled(&self, enabled: bool) -> Result<(), String>;
}

/// The system's login items, for this executable and data dir.
pub struct System {
    /// The entry's name: the file's stem, the LaunchAgent's label, the
    /// registry value's name.
    name: String,
    /// The command it runs.
    program: PathBuf,
    args: Vec<OsString>,
    /// The entry's file, on Linux and macOS; `None` if there is no home
    /// or config directory.
    #[cfg(not(windows))]
    file: Option<PathBuf>,
}

impl System {
    /// The entry for this executable, started on `data_dir` (`--data-dir`)
    /// if one was given.
    pub fn new(data_dir: Option<&Path>) -> Self {
        let name = entry_name(data_dir);
        // The installed name, should the running one be unknown: `ferry-gui`
        // on Linux, `Ferry` in the macOS bundle and on Windows.
        let installed = if cfg!(any(target_os = "macos", windows)) {
            "Ferry"
        } else {
            "ferry-gui"
        };
        let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(installed));
        let mut args = vec![OsString::from("--background")];
        if let Some(data_dir) = data_dir {
            let absolute = std::path::absolute(data_dir).unwrap_or_else(|_| data_dir.to_owned());
            args.extend([OsString::from("--data-dir"), absolute.into_os_string()]);
        }
        #[cfg(not(windows))]
        let file = entry_file(&name);
        Self {
            name,
            program,
            args,
            #[cfg(not(windows))]
            file,
        }
    }
}

/// [`APP_ID`] for the default data dir, else followed by a hash of the
/// absolute data dir.
fn entry_name(data_dir: Option<&Path>) -> String {
    let Some(data_dir) = data_dir else {
        return APP_ID.to_owned();
    };
    let absolute = std::path::absolute(data_dir).unwrap_or_else(|_| data_dir.to_owned());
    let hash = Sha256::digest(absolute.to_string_lossy().as_bytes());
    let hash: String = hash[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{APP_ID}-{hash}")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn entry_file(name: &str) -> Option<PathBuf> {
    // `$XDG_CONFIG_HOME/autostart`, `~/.config/autostart` without it.
    let config = directories::BaseDirs::new()?.config_dir().to_owned();
    Some(config.join("autostart").join(format!("{name}.desktop")))
}

#[cfg(target_os = "macos")]
fn entry_file(name: &str) -> Option<PathBuf> {
    let home = directories::BaseDirs::new()?.home_dir().to_owned();
    Some(
        home.join("Library/LaunchAgents")
            .join(format!("{name}.plist")),
    )
}

#[cfg(not(windows))]
impl LoginItem for System {
    fn is_enabled(&self) -> bool {
        let Some(file) = &self.file else {
            return false;
        };
        match std::fs::read_to_string(file) {
            Ok(contents) => !disabled_in(&contents),
            Err(_) => false,
        }
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let file = self.file.as_ref().ok_or("no home directory")?;
        let result = if enabled {
            let contents = entry(&self.name, &self.program, &self.args);
            file.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(file, contents))
        } else {
            match std::fs::remove_file(file) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                result => result,
            }
        };
        result.map_err(|error| error.to_string())
    }
}

/// An XDG autostart entry (Desktop Entry spec, and the Autostart spec's
/// `Hidden`).
#[cfg(all(unix, not(target_os = "macos")))]
fn entry(_name: &str, program: &Path, args: &[OsString]) -> String {
    let command: Vec<String> = std::iter::once(program.as_os_str())
        .chain(args.iter().map(OsString::as_os_str))
        .map(|arg| exec_arg(&arg.to_string_lossy()))
        .collect();
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Ferry\n\
         Comment=Start Ferry in the tray\n\
         Exec={}\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        command.join(" ")
    )
}

/// Whether a desktop environment's settings turned the entry off, rather
/// than removing it.
#[cfg(all(unix, not(target_os = "macos")))]
fn disabled_in(entry: &str) -> bool {
    entry.lines().any(|line| {
        let line = line.replace(' ', "");
        line == "Hidden=true" || line == "X-GNOME-Autostart-enabled=false"
    })
}

/// One argument of `Exec`: quoted if it holds a reserved character, with
/// `%` doubled, then escaped as a string value (which doubles `\` again).
#[cfg(all(unix, not(target_os = "macos")))]
fn exec_arg(arg: &str) -> String {
    const RESERVED: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(',
        ')', '`',
    ];
    let quoted = if arg.is_empty() || arg.contains(RESERVED) {
        let mut quoted = String::from("\"");
        for character in arg.chars() {
            if matches!(character, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(character);
        }
        quoted.push('"');
        quoted
    } else {
        arg.to_owned()
    };
    quoted.replace('%', "%%").replace('\\', "\\\\")
}

/// A LaunchAgent that runs the app once at login.
#[cfg(target_os = "macos")]
fn entry(name: &str, program: &Path, args: &[OsString]) -> String {
    let arguments: String = std::iter::once(program.as_os_str())
        .chain(args.iter().map(OsString::as_os_str))
        .map(|arg| format!("\t\t<string>{}</string>\n", xml(&arg.to_string_lossy())))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>{}</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         {arguments}\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>ProcessType</key>\n\
         \t<string>Interactive</string>\n\
         </dict>\n\
         </plist>\n",
        xml(name)
    )
}

/// A LaunchAgent is on while its file is there.
#[cfg(target_os = "macos")]
fn disabled_in(_entry: &str) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(windows)]
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Where Task Manager's Startup tab records an entry it turned off.
#[cfg(windows)]
const STARTUP_APPROVED: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

#[cfg(windows)]
impl LoginItem for System {
    fn is_enabled(&self) -> bool {
        use windows_registry::CURRENT_USER;
        let listed = CURRENT_USER
            .open(RUN)
            .and_then(|key| key.get_string(&self.name))
            .is_ok();
        // Its first byte is even while on (2 or 6), odd once turned off.
        let turned_off = CURRENT_USER
            .open(STARTUP_APPROVED)
            .and_then(|key| key.get_value(&self.name))
            .is_ok_and(|value| value.first().is_some_and(|byte| byte % 2 == 1));
        listed && !turned_off
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        use windows_registry::CURRENT_USER;
        let key = CURRENT_USER
            .create(RUN)
            .map_err(|error| error.to_string())?;
        if enabled {
            key.set_string(&self.name, command_line(&self.program, &self.args))
                .map_err(|error| error.to_string())?;
            // Clear a "turned off" Task Manager left, or it stays off.
            if let Ok(approved) = CURRENT_USER.open(STARTUP_APPROVED) {
                let _ = approved.remove_value(&self.name);
            }
            Ok(())
        } else if key.get_string(&self.name).is_ok() {
            key.remove_value(&self.name)
                .map_err(|error| error.to_string())
        } else {
            Ok(())
        }
    }
}

/// Each argument in double quotes where it needs them. Paths can't hold
/// `"` on Windows.
#[cfg(windows)]
fn command_line(program: &Path, args: &[OsString]) -> String {
    std::iter::once(program.as_os_str())
        .chain(args.iter().map(OsString::as_os_str))
        .map(|arg| {
            let arg = arg.to_string_lossy();
            if arg.is_empty() || arg.contains([' ', '\t']) {
                format!("\"{arg}\"")
            } else {
                arg.into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_data_dir_has_the_plain_name_and_others_their_own() {
        assert_eq!(entry_name(None), APP_ID);
        let one = entry_name(Some(Path::new("/tmp/one")));
        let two = entry_name(Some(Path::new("/tmp/two")));
        assert!(one.starts_with(&format!("{APP_ID}-")), "{one}");
        assert_ne!(one, two);
        assert_eq!(one, entry_name(Some(Path::new("/tmp/one"))));
    }

    #[cfg(unix)]
    #[test]
    fn the_entry_starts_in_the_background_on_the_same_data_dir() {
        let item = System::new(Some(Path::new("/tmp/my data")));
        assert_eq!(
            item.args,
            [
                OsString::from("--background"),
                "--data-dir".into(),
                "/tmp/my data".into()
            ]
        );
        assert_eq!(System::new(None).args, [OsString::from("--background")]);
    }

    #[cfg(not(windows))]
    #[test]
    fn turning_it_on_writes_the_entry_and_off_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("autostart/entry");
        let item = System {
            name: "entry".into(),
            program: "/opt/Ferry App/Ferry".into(),
            args: vec!["--background".into()],
            file: Some(file.clone()),
        };
        assert!(!item.is_enabled());
        item.set_enabled(false).unwrap();

        item.set_enabled(true).unwrap();
        assert!(item.is_enabled());
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("Ferry App")
        );
        item.set_enabled(true).unwrap();

        item.set_enabled(false).unwrap();
        assert!(!item.is_enabled());
        assert!(!file.exists());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn exec_quotes_what_needs_it() {
        let entry = entry(
            "x",
            Path::new("/opt/Ferry App/Ferry"),
            &["--data-dir".into(), "/home/me/100%$dir".into()],
        );
        assert!(
            entry.contains(r#"Exec="/opt/Ferry App/Ferry" --data-dir "/home/me/100%%\\$dir""#),
            "{entry}"
        );
        assert!(!disabled_in(&entry));
        assert!(disabled_in(&format!("{entry}Hidden=true\n")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_launch_agent_lists_each_argument() {
        let entry = entry("a&b", Path::new("/Applications/Ferry<App"), &[]);
        assert!(entry.contains("<string>a&amp;b</string>"));
        assert!(entry.contains("<string>/Applications/Ferry&lt;App</string>"));
    }

    #[cfg(windows)]
    #[test]
    fn the_command_line_quotes_paths_with_spaces() {
        assert_eq!(
            command_line(
                Path::new(r"C:\Program Files\Ferry\Ferry.exe"),
                &["--background".into()]
            ),
            r#""C:\Program Files\Ferry\Ferry.exe" --background"#
        );
    }
}
