# Push 文件分组 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Push 右栏提供 IDEA 的 Group By 菜单，可分别开关 Directory、Repository 和 Module；默认全选。

**Architecture:** 在 Push 面板保存会话选项，用现有文件树的稳定节点编号投影当前层级，再应用折叠。文件仍引用 Git 返回的原始变更；菜单不参与推送计划。沿用 Commit 的眼睛入口、键盘和鼠标习惯。

**Tech Stack:** Rust、Ratatui、Crossterm、系统 Git；无新依赖。

---

1. **分组与导航**（`src/push/files.rs`）：用同名文件覆盖八种组合、父节点导航和折叠后定位。按仓库、内容根、压缩目录的顺序投影；无目录时按文件名、路径平铺。运行 `cargo test --locked push::files::`。
2. **菜单与详情**（`src/push.rs`、`src/push/display.rs`、`src/push/view.rs`、`src/push/keymap.rs`、`src/shell.rs`）：工具栏眼睛和 `o` 打开菜单；方向键或鼠标选项，Space / Enter 立即切换，Esc 关闭。平铺时在文件名旁标父目录。保留同一文件的补丁和滚动位置，切到其他仓库／提交沿用分组。
3. **验收**（`src/push/view_tests.rs`、`src/push.rs`、`src/shell/help_tests.rs`）：验证三种图标模式、窄屏命中、菜单不会误勾仓库，真实临时仓中检查重名、重命名补丁及 Git 内容。更新 `README.md`、`docs/design.md`、`docs/prd.md` 和模块索引；执行 `cargo test --locked`、`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`。
