use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"))
}

fn cbindgen_bin() -> PathBuf {
    if let Ok(path) = std::env::var("CBINDGEN_BIN") {
        return PathBuf::from(path);
    }

    let home = std::env::var("HOME").expect("HOME is not set");
    Path::new(&home).join(".cargo/bin/cbindgen")
}

#[test]
fn checked_in_header_matches_generated_output() {
    if std::env::var("XIFTY_REQUIRE_CBINDGEN").as_deref() != Ok("1") {
        eprintln!(
            "skipping checked_in_header_matches_generated_output because cbindgen is not required in this test context"
        );
        return;
    }

    let root = workspace_root();
    let generated = root.join("target/xifty-generated.h");
    let checked_in = root.join("include/xifty.h");

    let status = Command::new(cbindgen_bin())
        .current_dir(&root)
        .args([
            "--config",
            "cbindgen.toml",
            "--crate",
            "xifty-ffi",
            "--output",
        ])
        .arg(&generated)
        .args(["--lang", "c"])
        .status()
        .expect("failed to run cbindgen");
    assert!(status.success(), "cbindgen generation failed");

    let generated_content = std::fs::read_to_string(&generated).expect("missing generated header");
    let checked_in_content =
        std::fs::read_to_string(&checked_in).expect("missing checked-in header");

    assert_eq!(generated_content, checked_in_content);
}

#[test]
fn c_harness_can_probe_and_extract() {
    let root = workspace_root();
    let target = target_dir(&root);
    let c_source = root.join("crates/xifty-ffi/tests/c_probe_extract.c");
    let binary = target.join("c_probe_extract_smoke");
    let fixture = root.join("fixtures/minimal/happy.jpg");

    let cargo_status = Command::new("cargo")
        .current_dir(&root)
        .args(["build", "-p", "xifty-ffi"])
        .status()
        .expect("failed to build xifty-ffi");
    assert!(cargo_status.success(), "cargo build -p xifty-ffi failed");

    let cc_status = Command::new("cc")
        .current_dir(&root)
        .arg(&c_source)
        .args(["-I", "include"])
        .arg("-L")
        .arg(target.join("debug"))
        .args(["-lxifty_ffi", "-o"])
        .arg(&binary)
        .status()
        .expect("failed to compile C harness");
    assert!(cc_status.success(), "C harness compilation failed");

    let run_status = Command::new(&binary)
        .arg(&fixture)
        .status()
        .expect("failed to execute C harness");
    assert!(run_status.success(), "C harness execution failed");
}
