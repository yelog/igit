# Log Paths Selection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Paths 菜单增加逐行输入和按仓库展开勾选两种入口，支持同时按多个文件或目录筛选真实提交。

**Architecture:** 在 Log 页的 Filter 中保存相对路径和可选的仓库 ID；每个仓库只将适用的路径传给 Git。菜单沿用立即生效的 Roots，两个选择器单独维护草稿，确认后才重载历史。树仅在展开目录时读取其直接子项。

**Tech Stack:** Rust、Ratatui、Crossterm、系统 Git；复用当前测试后端，不加依赖。

---

1. **先验证入口和范围。** 在 `src/log/view_tests/path_selection.rs` 用临时仓库创建同名文件、带方括号的文件和目录。通过渲染命中区域和键盘操作验证菜单顺序、逐行粘贴、多路径并集、树勾选的仓库边界、取消与页签恢复。先用 `cargo test --locked paths_menu_offers` 复现缺少入口。
2. **保存和查询路径。** 在 `src/log/model/path_filter.rs` 解析字面相对路径，在 `src/log/model.rs` 按仓构造 `git --literal-pathspecs log … -- path1 path2`。整仓不附加路径；只选其他仓的路径时跳过本仓历史。保留未限定名称的路径按每个勾选仓库匹配的能力。
3. **接入选择器。** 在 `src/log/paths.rs`、`src/log/paths/` 实现菜单、带光标的多行编辑器和目录树。树使用独立的箭头与复选框命中，支持部分勾选和键盘收展。用 `src/log.rs`、`src/log/filters.rs`、`src/log/view.rs` 路由绘制、按键、鼠标和粘贴，`src/log/keymap.rs` 提供对应帮助。
4. **验收。** 同步 README、交互设计、架构和 PRD。先运行 `cargo test --locked paths_`，再运行全量测试、格式和 Clippy；确认小屏仍能滚动到末仓，应用和取消均可点击。

### 验证记录（2026-11-04）

- 全量测试通过（182 项单元测试、3 项集成测试）；`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings` 和 `cargo build --locked --bin igit` 通过。
- 在 140×32 伪终端运行 Log 演练工作区：粘贴两行后用 `Ctrl-S` 应用，再用树选择 `atlas-web/src`。两个仓库的引用、index 条目和工作区状态在操作前后一致。
