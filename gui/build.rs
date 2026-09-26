//! Works out the version the app shows in Settings and About: the git tag
//! it was built from, e.g. `v1.2.0`. A release gets it from
//! `FERRY_VERSION` (the tag's number, which CI sets); a dev build from
//! `git describe`, which is the tag itself on a tagged commit and
//! `v1.2.0-2-g9e6caee` after one, or `dev` outside a git checkout. On
//! Windows it also embeds the exe's icon and names.

use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=FERRY_VERSION");
    let version = match env::var("FERRY_VERSION") {
        Ok(version) if !version.trim().is_empty() => tag(version.trim()),
        _ => describe().unwrap_or_else(|| "dev".to_owned()),
    };
    println!("cargo:rustc-env=FERRY_APP_VERSION={version}");
    #[cfg(windows)]
    windows_resources();
}

/// The icon Explorer, the taskbar and the installer's shortcuts show, and
/// the name Task Manager lists the app under.
#[cfg(windows)]
fn windows_resources() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = "../assets/windows/app_icon.ico";
    println!("cargo:rerun-if-changed={icon}");
    winresource::WindowsResource::new()
        .set_icon(icon)
        .set("FileDescription", "Ferry")
        .set("ProductName", "Ferry")
        .set("OriginalFilename", "Ferry.exe")
        .compile()
        .expect("could not embed the Windows resources");
}

/// A release's version as its tag: `1.2.0` is `v1.2.0`.
fn tag(version: &str) -> String {
    if version.starts_with(|c: char| c.is_ascii_digit()) {
        format!("v{version}")
    } else {
        version.to_owned()
    }
}

/// `git describe`, and a rerun whenever the commit it describes changes.
fn describe() -> Option<String> {
    let described = git(&["describe", "--tags", "--always"])?;
    // HEAD names the branch; the branch's ref (loose or packed) names the
    // commit. A worktree has its own HEAD but shares the refs.
    let git_dir = PathBuf::from(git(&["rev-parse", "--path-format=absolute", "--git-dir"])?);
    let common_dir = PathBuf::from(git(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?);
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    println!(
        "cargo:rerun-if-changed={}",
        common_dir.join("packed-refs").display()
    );
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        println!(
            "cargo:rerun-if-changed={}",
            common_dir.join(branch).display()
        );
    }
    Some(described)
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned()).filter(|text| !text.is_empty())
}
