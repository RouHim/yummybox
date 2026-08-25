use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Embed YUMMYBOX_VERSION at compile time for /api/version and the footer.
    // Precedence: env YUMMYBOX_VERSION (CI / Docker ARG) > git describe --tags > Cargo.toml fallback.
    // The Rust code uses `option_env!("YUMMYBOX_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))`,
    // so this build script only needs to set the rustc-env when it can derive a better version
    // than the shell env already provides.
    println!("cargo:rerun-if-env-changed=YUMMYBOX_VERSION");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags/");
    println!("cargo:rerun-if-changed=.git/packed-refs");
    let sanitized_env_version = std::env::var("YUMMYBOX_VERSION").ok().and_then(|v| {
        let sanitized = v.trim().trim_start_matches('v').trim().to_owned();
        if sanitized.is_empty() {
            None
        } else {
            Some(sanitized)
        }
    });
    if let Some(v) = sanitized_env_version {
        println!("cargo:rustc-env=YUMMYBOX_VERSION={v}");
    } else if let Some(v) = git_describe_version() {
        println!("cargo:rustc-env=YUMMYBOX_VERSION={v}");
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let web_dir = manifest_dir.join("web");

    // Build the frontend (skip if web/build/ already exists, e.g., pre-built for musl cross).
    // Both npm install and npm run build are skipped when a pre-built web/build/ is present,
    // because the musl build environment has no Node.js runtime.
    let build_dir = web_dir.join("build");
    if !build_dir.exists() {
        let node_modules = web_dir.join("node_modules");
        if !node_modules.exists() {
            let status = Command::new("npm")
                .args(["install"])
                .current_dir(&web_dir)
                .status()
                .expect("failed to run npm install in web/");
            if !status.success() {
                panic!("npm install failed with status: {status}");
            }
        }

        let status = Command::new("npm")
            .args(["run", "build"])
            .current_dir(&web_dir)
            .status()
            .expect("failed to run npm run build in web/");
        if !status.success() {
            panic!("npm run build failed with status: {status}");
        }
        if !build_dir.exists() {
            panic!("web/build/ does not exist after npm run build");
        }
    }

    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/package.json");
    println!("cargo:rerun-if-changed=web/static");
    println!("cargo:rerun-if-changed=web/build");
    println!("cargo:rerun-if-changed=migrations");
}

fn git_describe_version() -> Option<String> {
    // Prefer the highest version tag (covers feature branches that are behind the latest tag),
    // fall back to `git describe --tags --abbrev=0` for ancestry.
    let highest_tag = Command::new("git")
        .args(["tag", "--list", "v*", "--sort=-version:refname"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let first = String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .map(|s| s.trim().trim_start_matches('v').to_owned())?;
            if first.is_empty() { None } else { Some(first) }
        });
    if let Some(v) = highest_tag {
        return Some(v);
    }
    let output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stripped = raw.trim_start_matches('v').trim().to_owned();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}
