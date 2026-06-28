use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let web_dir = manifest_dir.join("web");

    // Install npm dependencies if node_modules is missing
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

    // Build the frontend (skip if web/build/ already exists, e.g., pre-built for musl cross)
    let build_dir = web_dir.join("build");
    if !build_dir.exists() {
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
}
