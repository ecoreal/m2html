<!-- ecoreal-repo-header: m2html -->
<div align="center">

<img src="assets/repo-icon.png" width="160" alt="M2HTML icon">

# m2html

**以 Rust 驱动的快速、安全的 Markdown 转 HTML 渲染器。**

[English](README.md) · 中文

批处理 CLI + 供应用代码使用的 Python 绑定。

</div>

`m2html` 是一个面向开发者的 Markdown 导出工具：Rust 内核、批处理 CLI、
以及供应用代码使用的 Python 绑定。v1 的目标是一个小而锋利的工具：渲染快、
默认安全、独立 HTML 页面观感好、安装简单。

## 特性

- 基于 `pulldown-cmark` 的 Rust 渲染内核。
- CLI 默认输出完整的独立 HTML 页面。
- Python API 默认输出可安全嵌入的 HTML 片段。
- Markdown 中的原始 HTML 默认被消毒（sanitize）。
- 导出文档自带 GitHub 风格的页面 CSS。
- CLI 页面输出支持围栏代码块的语法高亮。
- 递归目录导出并保持相对路径。

## 安装

```sh
cargo install m2html
```

```sh
pip install m2html
```

Python 包使用 PyO3 与 maturin 构建。

## CLI

把单个 Markdown 文件渲染到标准输出：

```sh
m2html README.md
```

把单个文件渲染为独立页面：

```sh
m2html README.md -o README.html
```

把整个目录渲染到 `site/`：

```sh
m2html docs/ -o site/
```

目录导出保持相对路径；当多个 Markdown 扩展名映射到同一个 `.html` 路径时
拒绝覆盖。冲突检查不区分大小写，以便跨文件系统移植；包含符号链接的输出
路径会被拒绝。

输出可嵌入片段而不是完整页面：

```sh
m2html README.md --fragment
```

只在信任输入时才允许原始 HTML：

```sh
m2html README.md --unsafe-html
```

查看全部可用选项：

```sh
m2html --help
```

## Python

```python
import m2html

html = m2html.render("# Hello")
assert html == "<h1>Hello</h1>\n"

page = m2html.render("# Hello", page=True, style="github", highlight=True)
```

渲染文件：

```python
import m2html

m2html.render_file("README.md", "README.html", page=True, style="github", highlight=True)
```

Python 默认值刻意保守：

- `page=False`
- `unsafe_html=False`
- `highlight=False`
- `style="none"`

## 安全

`m2html` 默认对渲染结果做 HTML 消毒，覆盖了 Markdown 输出被插入网站或
提供给其他用户的常见场景。

不可信输入同时受到上限约束：Markdown 输入不超过 16 MiB；渲染后的 HTML
超过 64 MiB 或嵌套深度超过 512 个元素时会被拒绝。深度嵌套或快速膨胀的
输入会让消毒过程变得非常慢，因此这两个上限同样作用于 `--unsafe-html`
的输出，且远高于手写 Markdown 能达到的量级。

只在 Markdown 可信时使用 `--unsafe-html` 或 `unsafe_html=True`。该模式会
保留输入中的原始 HTML，如果输出被提供给浏览器，可能暴露 XSS 漏洞。

## 基准测试

计划中的基准集合会把 Python 侧渲染与以下实现对比：

- `markdown-it-py`
- `mistune`
- `Python-Markdown`

基准应报告机器信息、文档大小、吞吐量和延迟。首个公开基准应包含小型
README、中型文档页面和大型生成 Markdown。

## 开发

构建全部组件：

```sh
cargo build --workspace
```

运行测试：

```sh
cargo test --workspace
```

本地构建 Python 扩展：

```sh
python -m pip install maturin
maturin develop
```

## 许可证

双许可，任选其一：

- Apache License, Version 2.0
- MIT License
