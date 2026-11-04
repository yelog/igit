# Commit / Git Log 分屏与操作弹窗 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Commit 在上、Git Log 在下，各占工作区一半；Push、Merge 作为模态窗口打开，移除全局面板页签。

**Architecture:** Shell 持有分屏焦点、当前操作窗口和上一帧的命中区域。两块常驻面板独立保留选择与滚动；帮助与写入审查覆盖当前窗口。复用原有 Git 计划、确认和后台任务。

**Tech Stack:** Rust、Ratatui、Crossterm、TestBackend、临时 Git 工作区。

---

1. 在 `src/shell/workspace_tests.rs` 中验证两个面板在同一帧有真实数据，上下等分（奇数行允许相差一行），鼠标与数字键选中正确面板，操作弹窗拦截底层事件，Esc 返回原处。先运行 `cargo test --locked split_workspace` 验证旧布局无法满足这些条件。
2. 在 `src/shell.rs` 和 `src/shell/workspace.rs` 分离工作区焦点与操作窗口，调整绘制顺序、事件路由与刷新时机。保留 Log 自己的筛选页签，编辑、帮助和确认先处理输入；Esc 从内到外返回。
3. 适配 `src/commit/view.rs`、`src/commit.rs`、`src/log.rs` 等面板的可用高度和浮层命中，在 44×12、80×24、120×36 检查短窗口里的清单、筛选、编辑器与弹窗。
4. 同步 `src/ui/help.rs`、`README.md`、`docs/design.md`、`docs/architecture.md`、`docs/prd.md`。运行 Shell 定向测试、全量 `cargo test --locked`、fmt 与 clippy；在临时多仓工作区用实际终端输入复核布局与弹窗返回。
