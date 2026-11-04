# Executable Help Palette Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 严格按 lazydb 的帮助样式与交互实现默认输入、上下选择、回车执行。

**Architecture:** 帮助改为固定 Search 输入和紧凑候选列表，沿用 lazydb 的 74 列、圆角、深蓝背景、青色标题、蓝色按键和 `>` 选择标记。将多动作摘要拆为独立静态按键序列，只列当前上下文可用的区域、面板通用和全局动作；Shell 恢复原状态并经既有事件路径执行，保留 Git 审查与验证。

**Tech Stack:** 现有 Rust / Ratatui / Crossterm / unicode-width，无新依赖。

---

### Task 1: 输入与候选交互

**Files:** `src/ui/help.rs`, `src/shell/help_tests.rs`.

1. 首先测试默认输入光标、中文编辑、方向选择、筛选后重置、空结果 Enter、可见区及鼠标选择。
2. 替换分组正文和浏览模式，按 lazydb 的渲染尺寸、色值、边框、列宽和底栏绘制。
3. 执行 `cargo test --locked ui::help::`。

### Task 2: 可执行快捷键

**Files:** `src/{log,commit,push,merge}/keymap.rs`, `src/log/branches.rs`, `src/shell.rs`.

1. 每行只表示一个明确动作，为多键序列、修饰键建立可验证的执行数据；对照 lazydb 只显示当前上下文可用的动作。
2. Enter 恢复原弹窗／编辑状态，校验上下文并派发原快捷键。
3. 测试实际导航、搜索、面板切换、多键序列、编辑动作及提交审查；取消和仅搜索时核对 HEAD、index、工作区不变。

### Task 3: 文档与验证

**Files:** `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`.

1. 更新 Search、选择执行和样式说明，替换上一版只读浏览的描述。
2. 检查 120×36、80×24、44×12 实际绘制结果。
3. 运行 `cargo fmt --all --check`、`cargo test --locked`、`cargo clippy --locked --all-targets -- -D warnings` 和 `git diff --check`。

直接执行，不提交或推送，不更改 lazydb。

## 实施与验收记录

- 已完成 lazydb 样式的默认 Search 焦点、紧凑单行候选、上下选择、回车执行、输入编辑、鼠标选择和滚动；当前区域、通用、全局动作按优先级去重。
- 19 项帮助测试通过，包括真实按键序列执行、原输入／弹窗恢复、上下文失效校验和提交审查边界。搜索文本不会成为执行数据，Git 写操作仍进入原审查路径。
- 全量 `cargo test --locked`：146 项单元测试、3 项集成测试通过。`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`、`git diff --check` 和文档相对链接检查通过。
- 已检查 120×36、80×24、44×12 的 TestBackend 实际绘制结果，包含默认输入光标与筛选后的候选标记；临时导出测试已移除。
- 合入工作区并行新增的 Commit View Options 帮助上下文，并调整其鼠标 match 分支以通过 Clippy；保留菜单内其他鼠标按钮的原有拦截行为，相应定向用例通过。
