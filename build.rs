use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    println!("cargo:rustc-env=LUM_BUILD_TIME_UTC={}", build_time());

    let hash = git_commit_hash_short().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=LUM_GIT_COMMIT_HASH_SHORT={hash}");
}

fn build_time() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%d %H:%M:%S UTC")
        .to_string()
}

fn git_commit_hash_short() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
