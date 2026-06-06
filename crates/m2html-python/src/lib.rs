use std::{fs, path::PathBuf};

use m2html_core::{PageStyle, RenderOptions};
use pyo3::{exceptions::PyValueError, prelude::*};

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

    m2html_core::render(markdown, &options).map_err(|err| PyValueError::new_err(err.to_string()))
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
    let markdown = fs::read_to_string(&input_path).map_err(|err| {
        PyValueError::new_err(format!("failed to read {}: {err}", input_path.display()))
    })?;
    let options = options_from_python(page, unsafe_html, highlight, style, title)?;
    let html = m2html_core::render(&markdown, &options)
        .map_err(|err| PyValueError::new_err(err.to_string()))?;

    if let Some(path) = output_path {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|err| {
                PyValueError::new_err(format!("failed to create {}: {err}", parent.display()))
            })?;
        }

        fs::write(&path, &html).map_err(|err| {
            PyValueError::new_err(format!("failed to write {}: {err}", path.display()))
        })?;
    }

    Ok(html)
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

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add_function(wrap_pyfunction!(render_file, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
