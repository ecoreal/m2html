use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use m2html_core::{MAX_MARKDOWN_BYTES, PageStyle, RenderError, RenderOptions};
use pyo3::{
    exceptions::{PyOSError, PyUnicodeDecodeError, PyValueError},
    prelude::*,
};
use tempfile::NamedTempFile;

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
    py: Python<'_>,
    input_path: PathBuf,
    output_path: Option<PathBuf>,
    page: bool,
    unsafe_html: bool,
    highlight: bool,
    style: &str,
    title: Option<String>,
) -> PyResult<String> {
    let markdown = read_markdown(py, &input_path)?;
    let options = options_from_python(page, unsafe_html, highlight, style, title)?;
    let html = m2html_core::render(&markdown, &options).map_err(render_error_to_python)?;

    if let Some(path) = output_path {
        write_atomic(&path, &html)?;
    }

    Ok(html)
}

fn read_markdown(py: Python<'_>, path: &Path) -> PyResult<String> {
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

    match String::from_utf8(bytes) {
        Ok(markdown) => Ok(markdown),
        Err(error) => {
            Err(PyUnicodeDecodeError::new_utf8(py, error.as_bytes(), error.utf8_error())?.into())
        }
    }
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
    let mut temporary = NamedTempFile::new_in(parent)
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
            let error = read_markdown(py, &path).unwrap_err();
            assert!(error.is_instance_of::<PyOSError>(py));
        });
    }

    #[test]
    fn maps_invalid_utf8_to_unicode_decode_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("invalid.md");
        fs::write(&path, [0xff, 0xfe]).unwrap();

        Python::attach(|py| {
            let error = read_markdown(py, &path).unwrap_err();
            assert!(error.is_instance_of::<PyUnicodeDecodeError>(py));
        });
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add_function(wrap_pyfunction!(render_file, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
