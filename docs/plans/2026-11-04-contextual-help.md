# Contextual Help Implementation Plan

> 初版记录；帮助的现行样式与选择执行规则见 [Executable Help Palette](2026-11-04-executable-help-palette.md)。

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 参考 lazydb，将 IGit 的帮助改为按当前焦点排序、可搜索和滚动的快捷键清单。

**Architecture:** 各面板通过内部 `keymap` 方法提供当前区域、面板通用和其他区域的结构化帮助；Shell 在打开帮助时捕获上下文，并接入独立的 `ui/help.rs` 渲染与事件处理，不改变公开的 `Panel` trait。帮助只浏览，不执行列出的命令；从输入框或 Shell 弹窗用 F1 打开后，关闭时恢复原来的状态。

**Tech Stack:** Rust、Ratatui 0.29、Crossterm 0.28、现有 unicode-width；不增加依赖。

---

### Task 1: 帮助模型与清单界面

**Files:** Create `src/ui/help.rs`; modify `src/ui.rs`.

1. 定义快捷键分组、上下文和帮助会话；保留现有 `Panel::help` 的底部摘要用途。
2. 实现当前区域优先、中文/按键/分组搜索、空结果提示、j/k/方向键/翻页/首尾和鼠标滚动。
3. 快捷键和说明分列；窄屏换行；显示当前上下文、匹配计数、滚动位置和关闭入口。
4. 运行 `cargo test --locked ui::help::`，覆盖筛选、长内容、窄屏、缩放和鼠标命中。

### Task 2: 真实焦点与帮助入口

**Files:** Create `src/log/keymap.rs`, `src/commit/keymap.rs`, `src/push/keymap.rs`, `src/merge/keymap.rs`; modify corresponding panel module declarations and `src/shell.rs`.

1. 逐个对照事件分支登记四屏的区域快捷键，包括 Diff、筛选、消息输入及合并结果编辑。
2. `?` 保持导航模式入口；`F1` 可从输入或弹窗打开；点击顶栏帮助保留当前焦点。
3. Shell 暂存被覆盖的弹窗；关闭帮助恢复弹窗、草稿与焦点；帮助内输入不分发给原面板。
4. 用 Shell 和面板测试检查上下文顺序、输入字符隔离及确认框恢复，确保帮助不会触发 Git 写入。

### Task 3: 文档与完整验证

**Files:** Modify `README.md`, `docs/design.md`, `docs/architecture.md`, `docs/prd.md`.

1. 更新入口、搜索/滚动操作与上下文规则，记录可重复的验收方式。
2. 运行 `cargo fmt --all --check`、`cargo test --locked` 和 `cargo clippy --locked --all-targets -- -D warnings`。
3. 使用 TestBackend 和本地临时演练仓检查 120×36、80×24、44×12 下的呈现与只读性。

按当前请求直接执行以上步骤；不创建提交、不推送，不更改 lazydb。

## 实施与验收记录

- 已完成区域帮助、搜索、键盘与鼠标滚动、滚动条点击／拖动、窄屏换行和 F1 浮层恢复；公开 `Panel` trait 与依赖不变。
- 新增 12 项帮助回归测试，包括四屏焦点、鼠标入口、草稿／审查恢复、输入光标、真实合并编辑，以及 HEAD、index 与工作区只读性。
- `cargo test --locked`：133 项单元测试、3 项集成测试全部通过；`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings` 和 `git diff --check` 通过。
- 已检查 TestBackend 的 120×36、80×24、44×12 实际绘制结果；渲染仅用于界面验证，不代表手动伪终端走查。
