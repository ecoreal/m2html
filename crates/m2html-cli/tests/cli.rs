use std::{fs, path::PathBuf, process::Command};

use m2html_core::{MAX_MARKDOWN_BYTES, MAX_NESTING_DEPTH};
use tempfile::tempdir;

#[cfg(unix)]
fn symlink_directory(original: &std::path::Path, link: &std::path::Path) {
    std::os::unix::fs::symlink(original, link).expect("create symlink");
}

#[cfg(windows)]
fn symlink_directory(original: &std::path::Path, link: &std::path::Path) {
    std::os::windows::fs::symlink_dir(original, link).expect("create symlink");
}

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
fn rejects_symlinked_single_file_output() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.md");
    fs::write(&input, "# Input").expect("write input");
    let external = dir.path().join("external.html");
    fs::write(&external, "keep").expect("write external");
    let output = dir.path().join("output.html");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external, &output).expect("create symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&external, &output).expect("create symlink");

    let result = Command::new(m2html_binary())
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run m2html");

    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).expect("utf8 stderr");
    assert!(stderr.contains("symlink"), "stderr: {stderr}");
    assert_eq!(
        fs::read_to_string(&external).expect("read external"),
        "keep"
    );
}

#[test]
fn rejects_case_insensitive_directory_output_collisions() {
    let dir = tempdir().expect("tempdir");
    let docs = dir.path().join("docs");
    fs::create_dir_all(&docs).expect("create docs");
    fs::write(docs.join("Guide.md"), "# Guide").expect("write input");
    fs::write(docs.join("guide.markdown"), "# Alias").expect("write input");

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

#[test]
fn rejects_symlinked_output_directory() {
    let dir = tempdir().expect("tempdir");
    let docs = dir.path().join("docs");
    fs::create_dir_all(&docs).expect("create docs");
    fs::write(docs.join("guide.md"), "# Guide").expect("write input");

    let external = dir.path().join("external");
    fs::create_dir(&external).expect("create external");
    let output_dir = dir.path().join("site");
    symlink_directory(&external, &output_dir);

    let output = Command::new(m2html_binary())
        .arg(&docs)
        .arg("-o")
        .arg(&output_dir)
        .output()
        .expect("run m2html");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("symlink"), "stderr: {stderr}");
    assert!(!external.join("guide.html").exists());
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

#[test]
fn rejects_oversized_markdown_input_before_writing_output() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("large.md");
    fs::write(&input, vec![b'x'; MAX_MARKDOWN_BYTES + 1]).expect("write input");
    let output = dir.path().join("output.html");
    fs::write(&output, "keep").expect("write existing output");

    let result = Command::new(m2html_binary())
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run m2html");

    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).expect("utf8 stderr");
    assert!(stderr.contains("maximum is"), "stderr: {stderr}");
    assert_eq!(fs::read_to_string(output).expect("read output"), "keep");
}

#[test]
fn rejects_deeply_nested_markdown_before_writing_output() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("deep.md");
    fs::write(&input, format!("{}x\n", "> ".repeat(MAX_NESTING_DEPTH + 1))).expect("write input");
    let output = dir.path().join("output.html");
    fs::write(&output, "keep").expect("write existing output");

    let result = Command::new(m2html_binary())
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run m2html");

    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).expect("utf8 stderr");
    assert!(stderr.contains("nests deeper"), "stderr: {stderr}");
    assert_eq!(fs::read_to_string(output).expect("read output"), "keep");
}

#[cfg(unix)]
#[test]
fn writes_output_files_with_default_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.md");
    fs::write(&input, "# Permissions").expect("write input");
    let output = dir.path().join("output.html");

    let result = Command::new(m2html_binary())
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("run m2html");

    assert!(result.status.success());
    let mode = fs::metadata(&output)
        .expect("stat output")
        .permissions()
        .mode()
        & 0o777;
    // `NamedTempFile` creates owner-only files; write_atomic must publish the
    // output with the mode a regular file would get (0o666 masked by the umask).
    assert_ne!(mode, 0o600, "output kept the temporary file permissions");
    assert_eq!(mode & 0o044, 0o044, "output is not group/other readable");
}

#[test]
fn renders_nested_markdown_at_the_depth_limit() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.md");
    fs::write(&input, format!("{}x\n", "> ".repeat(16))).expect("write input");

    let result = Command::new(m2html_binary())
        .arg(&input)
        .arg("--fragment")
        .output()
        .expect("run m2html");

    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).expect("utf8 stdout");
    assert_eq!(stdout.matches("<blockquote>").count(), 16);
}

#[test]
fn readmes_cross_link_and_keep_section_parity() {
    // The repo ships English and Chinese READMEs; they must stay
    // cross-linked and structurally aligned, and the Chinese file must be
    // reachable from the repository root (it is listed in the header).
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let en = fs::read_to_string(root.join("README.md")).expect("read README.md");
    let zh = fs::read_to_string(root.join("README.zh.md")).expect("read README.zh.md");

    assert!(
        en.contains("[中文](README.zh.md)"),
        "English README lacks the Chinese link"
    );
    assert!(
        zh.contains("[English](README.md)"),
        "Chinese README lacks the English link"
    );

    let en_sections = en.lines().filter(|line| line.starts_with("## ")).count();
    let zh_sections = zh.lines().filter(|line| line.starts_with("## ")).count();
    assert_eq!(en_sections, zh_sections, "README section counts diverged");
}
