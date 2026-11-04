# Git Log View Options Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在原生 Log 区右上角实现 IDEA 眼睛菜单中的显示项、Columns 子菜单和提交高亮。

**Architecture:** 将显示偏好和菜单状态集中到 `src/log/display.rs`，沿用 Log 的绘制、命中与模态输入分发。历史查询同时取得作者邮箱与两种日期；签名和 cherry 等价检查仅在启用时读取。显示选项在同一会话的 Log 页间共享，切换选项不改变筛选、提交顺序或选中对象，不写 Git 状态，不新增依赖。

**Tech Stack:** Rust、Ratatui TestBackend、Crossterm、系统 Git。

---

### Task 1: 菜单入口与键鼠交互

**Files:** `src/log/display.rs`、`src/log.rs`、`src/log/view.rs`、`src/log/filters.rs`、`src/log/view_tests.rs`、`src/shell.rs`。

1. 添加基于 TestBackend 的用例，检查 48/80/120/180 列下眼睛入口、菜单分组与 Columns 子菜单；运行 `cargo test --locked log::view_tests::view_options`，确认新增功能尚不存在。
2. 在图工具栏右端预留眼睛按钮；单区模式放在区域导航右端。`o` 打开，`j/k` 选择，`l/Right` 进子菜单、`h/Left` 返回，`Space/Enter` 切换，`Esc` 返回或关闭。鼠标点击、悬停与键盘共用状态。
3. 窄窗口将子菜单放在主菜单左侧或同一位置；短窗口滚动当前菜单。菜单表面拦截底下的提交；菜单外点击关闭并继续处理该次点击；Shell 头部仍可直接切换面板或 Roots。

### Task 2: 真实显示与高亮

**Files:** `src/log/model.rs`、`src/log/model/tests.rs`、`src/log/model/presentation.rs`、`src/log/view.rs`、`src/log/model/graph.rs`、`src/log/model/graph/render.rs`、`src/log/view_tests.rs`。

1. 读取 `%ae`、`%ad`、`%cd`；保留 `%ct` 的排序语义。按仓身份匹配 My Commits；签名列启用后读取 `%G?` 并区分有效、无签名、缺密钥、过期与无效签名。
2. 实现 Root Names、Compact References View、Tag Names、References on the Left；关闭标签名仍保留标签图标。独立控制 Author、Hash、Date、GPG Signature，保证提交标题保有空间。
3. 关闭 Long Edges 时以箭头缩短超过阈值的分支线，箭头可跳至另一端；中途有其他分支汇入的共享轨道保持完整。开启时恢复完整轨道，过滤缺口不伪造边。
4. My Commits 粗体、Merge Commits 降亮度、Current Branch 蓝底独立开关。选择另一分支后用 `git cherry HEAD <branch>` 区分已经等价应用的提交，降低已应用项亮度；无可比较分支时明确显示不可用提示。
5. 使用临时真实仓库验证两种日期、逐仓身份、签名、cherry 等价项与同 SHA 跨仓隔离，检查显示切换保持当前选择与 Git 状态。

### Task 3: 文档与回归

**Files:** `README.md`、`docs/design.md`、`docs/prd.md`、`docs/architecture.md`。

1. 记录菜单项目、键鼠操作、默认值、会话共享、窄屏退让及签名/cherry 的按需查询边界。
2. 运行 `cargo test --locked log::`，修正相关回归。
3. 运行 `cargo test --locked`、`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`，并用临时演练历史检查实际终端显示；不执行 commit 或 push。
