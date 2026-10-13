use std::process::Command;

fn git_output(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
}

fn main() {
    for pathspec in ["HEAD", "packed-refs"] {
        if let Some(path) = git_output(&["rev-parse", "--git-path", pathspec]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(branch) = git_output(&["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        let ref_path = format!("refs/heads/{branch}");
        if let Some(path) = git_output(&["rev-parse", "--git-path", &ref_path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    println!(
        "cargo:rustc-env=WORKCTL_GIT_SHA={}",
        git_output(&["rev-parse", "HEAD"]).unwrap_or_default()
    );
}
