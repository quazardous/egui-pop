//! The commit the gallery is built from, for its version
//! (`EGUI_POP_GALLERY_COMMIT`, read by `pages::about`): git's short hash,
//! else the `EGUI_POP_GALLERY_COMMIT` variable, else nothing (a build
//! without git still works).

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=EGUI_POP_GALLERY_COMMIT");
    let commit = git(&["rev-parse", "--short", "HEAD"])
        .inspect(|_| watch_git())
        .or_else(|| std::env::var("EGUI_POP_GALLERY_COMMIT").ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=EGUI_POP_GALLERY_COMMIT={}", commit.trim());
}

/// Runs git in the crate's folder; `None` when git is missing or fails.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (out.status.success() && !text.is_empty()).then(|| text.to_string())
}

/// Builds again when HEAD moves: HEAD itself, the branch it names and the
/// packed refs (paths from git, so a worktree's work too).
fn watch_git() {
    let mut paths = vec!["HEAD".to_string(), "packed-refs".to_string()];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        paths.push(branch);
    }
    for path in paths {
        if let Some(file) = git(&["rev-parse", "--git-path", &path]) {
            let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file);
            if file.exists() {
                println!("cargo:rerun-if-changed={}", file.display());
            }
        }
    }
}
