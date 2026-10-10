use std::{fs, path::PathBuf, process::Command};

use tempfile::tempdir;

fn m2html_binary() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_m2html") {
        return PathBuf::from(path);
    }

    // Cargo does not expose CARGO_BIN_EXE_* consistently for every workspace
    // test invocation. The integration test binary lives in target/debug/deps,
    // so its sibling target/debug/m2html is a stable fallback.
    std::env::current_exe()
        .expect("current test executable")
        .parent()
        .and_then(|path| path.parent())
        .expect("target debug directory")
        .join("m2html")
}

#[test]
fn renders_fragment_to_stdout() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.md");
    fs::write(&input, "# Hello\n\n<script>alert(1)</script>").expect("write input");

    let output = Command::new(m2html_binary())
        .arg(&input)
        .arg("--fragment")
        .output()
        .expect("run m2html");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("<h1>Hello</h1>"));
    assert!(!stdout.contains("<script>"));
}

#[test]
fn renders_directory_preserving_relative_paths() {
    let dir = tempdir().expect("tempdir");
    let docs = dir.path().join("docs/nested");
    fs::create_dir_all(&docs).expect("create docs");
    fs::write(docs.join("guide.md"), "# Guide").expect("write input");

    let output_dir = dir.path().join("site");
    let output = Command::new(m2html_binary())
        .arg(dir.path().join("docs"))
        .arg("-o")
        .arg(&output_dir)
        .output()
        .expect("run m2html");

    assert!(output.status.success());
    let html = fs::read_to_string(output_dir.join("nested/guide.html")).expect("read html");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<h1>Guide</h1>"));
}

#[test]
fn rejects_directory_output_collisions() {
    let dir = tempdir().expect("tempdir");
    let docs = dir.path().join("docs");
    fs::create_dir_all(&docs).expect("create docs");
    fs::write(docs.join("guide.md"), "# Markdown").expect("write md input");
    fs::write(docs.join("guide.markdown"), "# Markdown alias").expect("write markdown input");

    let output_dir = dir.path().join("site");
    let output = Command::new(m2html_binary())
        .arg(&docs)
        .arg("-o")
        .arg(&output_dir)
        .output()
        .expect("run m2html");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("same output"), "stderr: {stderr}");
}
