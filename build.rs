use std::env;
use std::process::Command;

fn main() {
    let git_sha = env::var("GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(git_sha_from_git)
        .unwrap_or_else(|| "unknown".into());

    let built_at = env::var("BUILT_AT")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=GIT_SHA={git_sha}");
    println!("cargo:rustc-env=BUILT_AT={built_at}");
    println!("cargo:rerun-if-env-changed=GIT_SHA");
    println!("cargo:rerun-if-env-changed=BUILT_AT");
    println!("cargo:rerun-if-changed=.git/HEAD");
}

fn git_sha_from_git() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}
