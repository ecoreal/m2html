use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    string::FromUtf8Error,
};

use m2html_core::{MAX_MARKDOWN_BYTES, PageStyle, RenderError, RenderOptions};
use pyo3::{
    exceptions::{PyOSError, PyUnicodeDecodeError, PyValueError},
    prelude::*,
};

#[pyfunction]
#[pyo3(signature = (markdown, *, page = false, unsafe_html = false, highlight = false, style = "none", title = None))]
fn render(
    markdown: &str,
    page: bool,
    unsafe_html: bool,
    highlight: bool,
    style: &str,
    title: Option<String>,
) -> PyResult<String> {
    let options = options_from_python(page, unsafe_html, highlight, style, title)?;

    m2html_core::render(markdown, &options).map_err(render_error_to_python)
}

#[pyfunction]
#[pyo3(signature = (input_path, output_path = None, *, page = false, unsafe_html = false, highlight = false, style = "none", title = None))]
fn render_file(
    input_path: PathBuf,
    output_path: Option<PathBuf>,
    page: bool,
    unsafe_html: bool,
    highlight: bool,
    style: &str,
    title: Option<String>,
) -> PyResult<String> {
    let markdown = read_markdown(&input_path)?;
    let options = options_from_python(page, unsafe_html, highlight, style, title)?;
    let html = m2html_core::render(&markdown, &options).map_err(render_error_to_python)?;

    if let Some(path) = output_path {
        write_atomic(&path, &html)?;
    }

    Ok(html)
}

fn read_markdown(path: &Path) -> PyResult<String> {
    let file = File::open(path).map_err(|error| io_error("read", path, error))?;
    let mut bytes = Vec::new();
    file.take((MAX_MARKDOWN_BYTES as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read", path, error))?;

    if bytes.len() > MAX_MARKDOWN_BYTES {
        return Err(PyValueError::new_err(format!(
            "Markdown input {} is too large: maximum is {} bytes",
            path.display(),
            MAX_MARKDOWN_BYTES
        )));
    }

    String::from_utf8(bytes).map_err(invalid_utf8_error)
}

fn invalid_utf8_error(error: FromUtf8Error) -> PyErr {
    // Rebuild the GIL-bound decode error without threading a Python token
    // through every read helper.
    Python::attach(|py| {
        match PyUnicodeDecodeError::new_utf8(py, error.as_bytes(), error.utf8_error()) {
            Ok(decode_error) => decode_error.into(),
            Err(err) => err,
        }
    })
}

fn write_atomic(path: &Path, contents: &str) -> PyResult<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| io_error("create output directory", parent, error))?;
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut builder = tempfile::Builder::new();
    // `NamedTempFile` defaults to owner-only permissions (0o600 on Unix), which
    // would leak onto every rendered file once it is persisted. Ask for the
    // mode a regular file would get (0o666 masked by the umask) instead.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    let mut temporary = builder
        .tempfile_in(parent)
        .map_err(|error| io_error("create temporary output", parent, error))?;
    temporary
        .write_all(contents.as_bytes())
        .map_err(|error| io_error("write temporary output", path, error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| io_error("flush temporary output", path, error))?;
    temporary
        .persist(path)
        .map_err(|error| io_error("atomically replace output", path, error.error))?;
    Ok(())
}

fn io_error(operation: &str, path: &Path, error: std::io::Error) -> PyErr {
    PyOSError::new_err(format!("failed to {operation} {}: {error}", path.display()))
}

fn render_error_to_python(error: RenderError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn options_from_python(
    page: bool,
    unsafe_html: bool,
    highlight: bool,
    style: &str,
    title: Option<String>,
) -> PyResult<RenderOptions> {
    let style = PageStyle::parse(style).ok_or_else(|| {
        PyValueError::new_err("style must be one of: 'github', 'minimal', 'none'")
    })?;

    Ok(RenderOptions {
        page,
        unsafe_html,
        highlight,
        style,
        title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_markdown() {
        let markdown = "x".repeat(MAX_MARKDOWN_BYTES + 1);

        Python::attach(|py| {
            let error = render(&markdown, false, false, false, "none", None).unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(error.to_string().contains("maximum of"));
        });
    }

    #[test]
    fn maps_missing_input_to_os_error() {
        Python::attach(|py| {
            let path = PathBuf::from("definitely-missing-m2html-input.md");
            let error = read_markdown(&path).unwrap_err();
            assert!(error.is_instance_of::<PyOSError>(py));
        });
    }

    #[test]
    fn maps_invalid_utf8_to_unicode_decode_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("invalid.md");
        fs::write(&path, [0xff, 0xfe]).unwrap();

        Python::attach(|py| {
            let error = read_markdown(&path).unwrap_err();
            assert!(error.is_instance_of::<PyUnicodeDecodeError>(py));
        });
    }

    #[test]
    fn rejects_deeply_nested_markdown() {
        let markdown = format!("{}x\n", "> ".repeat(m2html_core::MAX_NESTING_DEPTH + 1));

        Python::attach(|py| {
            let error = render(&markdown, false, false, false, "none", None).unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(error.to_string().contains("nests deeper"));
        });
    }

    #[test]
    fn maps_deeply_nested_files_to_value_error() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("deep.md");
        fs::write(
            &input,
            format!("{}x\n", "> ".repeat(m2html_core::MAX_NESTING_DEPTH + 1)),
        )
        .unwrap();

        Python::attach(|py| {
            let error =
                render_file(input.clone(), None, false, false, false, "none", None).unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
        });
    }

    #[cfg(unix)]
    #[test]
    fn writes_output_files_with_default_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.md");
        let output = directory.path().join("output.html");
        fs::write(&input, "# Permissions").unwrap();

        Python::attach(|_py| {
            render_file(
                input,
                Some(output.clone()),
                false,
                false,
                false,
                "none",
                None,
            )
            .unwrap();
        });

        let mode = fs::metadata(&output).unwrap().permissions().mode() & 0o777;
        // `NamedTempFile` creates owner-only files; write_atomic must publish
        // the output with the mode a regular file would get.
        assert_ne!(mode, 0o600, "output kept the temporary file permissions");
        assert_eq!(mode & 0o044, 0o044, "output is not group/other readable");
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add_function(wrap_pyfunction!(render_file, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
