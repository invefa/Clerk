use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-env-changed=CLERK_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=CLERK_BUILD_DIRTY");
    // Observe index and refs without depending on the runtime checkout.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/index");
        if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
            && let Some(path) = git(&[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                &reference,
            ])
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(dir) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
        println!("cargo:rerun-if-changed={dir}/packed-refs");
    }
    if let Some(files) = git(&["ls-files", "-z"]) {
        for path in files.split('\0').filter(|path| !path.is_empty()) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let commit = std::env::var("CLERK_BUILD_COMMIT")
        .ok()
        .or_else(|| git(&["rev-parse", "HEAD"]));
    let dirty = std::env::var("CLERK_BUILD_DIRTY").ok().or_else(|| {
        git(&["status", "--porcelain", "--untracked-files=normal"])
            .map(|status| (!status.is_empty()).to_string())
    });
    if let Some(value) = commit {
        println!("cargo:rustc-env=CLERK_BUILD_COMMIT={value}");
    }
    if let Some(value) = dirty {
        println!("cargo:rustc-env=CLERK_BUILD_DIRTY={value}");
    }
}
