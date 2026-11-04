# 工作区 pane 焦点和尺寸 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 用边框标出实际接收输入的区域，用 Tab、Ctrl-W 方向键和拖动边框在同一套分屏内操作。

**Architecture:** 面板绘制时登记可访问的 pane 与实际分隔线。Shell 只在当前工作区或操作窗口内查找目标；尺寸、拖动捕获和未完成的组合键只保留在会话中。调整布局不改变仓库选择，也不执行 Git。

**Tech Stack:** Rust、Ratatui、Crossterm、TestBackend、本地演练仓。

---

1. 在 `src/shell/workspace_tests.rs` 走完整按键和鼠标路径：跨 Commit / Log 循环、方向移动、越界停留、键鼠调尺寸、还原、弹窗与编辑隔离；用 `cargo test --locked pane_navigation` 验证旧行为。
2. 在 `src/ui/panes.rs` 保存每帧 pane、分隔线和有效尺寸；命中与键盘调整共享上下限。布局测试覆盖缩窗、抵达边界后反向调整以及拖动捕获。
3. 在 Commit、Log、Push、Merge 的视图登记实际区域；更新边框样式、分隔线、留白命中与紧凑布局。各面板保留原选择、快捷键和页签。
4. 在 `src/shell/workspace.rs`、`src/shell.rs` 接入跨分屏焦点、拖动和工具弹窗优先级；`src/ui/key_sequence.rs`、`src/ui/help.rs` 支持可执行的 Ctrl-W 后续键。
5. 同步 README、设计、PRD 与架构说明，运行 `cargo fmt --all --check`、`cargo test --locked`、`cargo clippy --locked --all-targets -- -D warnings`，在临时演练仓中运行终端冒烟。
