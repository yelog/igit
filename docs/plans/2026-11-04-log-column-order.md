# Git Log Column Order Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在 Log 的 Columns 子菜单显示、隐藏和移动 Author、Hash、Date、GPG Signature、Graph 和 Message，键盘和鼠标均可操作。

**Architecture:** 会话级显示选项保存包含隐藏列的完整顺序，菜单焦点按列身份跟随移动。仓库标记固定在行首，六个列共用一套布局：先计算元数据和拓扑图的宽度，再把剩余宽度分给 Message，最后按菜单顺序绘制。图的跳转命中属于 Graph，标题和引用属于 Message；移动和切换这两列只改变显示状态。

**Tech Stack:** Rust、Ratatui、Crossterm、TestBackend、临时 Git 仓库。

---

### Task 1: 用实际输入和渲染定义验收条件

- 在 `src/log/view_tests/column_order.rs` 中通过 Columns 移动可见列和隐藏列；比较菜单行与提交行的文字位置，检查移动后的高亮、复选框、边界和当前提交。由 `src/log/view_tests.rs` 注册用例。
- 测试点击行尾按钮与点击列名的不同结果；在窄屏、短窗口和切换 Log 页后复验顺序。先运行 `cargo test --locked view_options_reorder`，确认现有固定布局不能通过。

### Task 2: 接通会话偏好、菜单和日志行

- 在 `src/log/display.rs` 中用固定长度的有序列设置同时承载身份与显隐；相邻交换不越过首尾。菜单用列身份处理悬停、点击、勾选和移动。
- 在 Columns 内，`j/k` 和方向键只浏览，`K/J`、`Shift+Up/Down` 及每行的上下按钮向前／后移动。鼠标操作与键盘走同一次交换，不调用 Git 或重载历史。
- 在 `src/log/view.rs` 中按现有规则先算每种列的宽度，再按偏好顺序绘制；在 `src/log/keymap.rs` 和菜单底部显示相应操作。
- 复验 Graph 与 Message 位于首、中、末位及隐藏时的实际日志行：标题与引用不侵占后续列，旧图位置不再响应跳转，隐藏图后不保留箭头命中。

### Task 3: 文档与验证

- 同步 `README.md`、`docs/design.md`、`docs/prd.md` 中的使用方式、默认值、会话范围与验收条件。
- 运行 `cargo test --locked view_options` 检查列调序与原菜单行为，再运行 `cargo test --locked`、`cargo fmt --all --check` 和 `cargo clippy --locked --all-targets -- -D warnings`。
