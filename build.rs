use std::path::PathBuf;
use std::process::Command;

fn main() {
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
}
