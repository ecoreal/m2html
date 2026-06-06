use std::{fs, process::Command};

use tempfile::tempdir;

#[test]
fn renders_fragment_to_stdout() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.md");
    fs::write(&input, "# Hello\n\n<script>alert(1)</script>").expect("write input");

    let output = Command::new(env!("CARGO_BIN_EXE_m2html"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_m2html"))
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
