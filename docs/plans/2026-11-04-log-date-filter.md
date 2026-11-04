# Git Log Date Filter Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 Log 的 Date 菜单改为 `Select...`、`Last 24 hours` 和 `Last 7 days`，首项编辑自定义起止日期。

**Architecture:** 在现有筛选菜单中为日期增加固定选项和双字段范围编辑，沿用菜单的取消、清除及页签恢复行为。日期条件用独立枚举表示，由现有 Git 日志查询生成相对时间或本地日期边界，不增加依赖。

**Tech Stack:** Rust、Ratatui TestBackend、Crossterm、系统 Git。

---

### Task 1: 固定菜单与范围输入

**Files:** `src/log.rs`、`src/log/filters.rs`、`src/log/view.rs`、`src/log/view_tests.rs`。

1. 添加渲染及输入测试，要求 Date 菜单只按截图顺序显示三个选项；运行 `cargo test --locked log::view_tests::date_menu`，确认旧实现失败。
2. 菜单默认选中首项；键盘方向键 / `j/k` 和鼠标均可选择。首项进入 From / To 编辑，格式为 `YYYY-MM-DD`，`Tab` 切换、`Ctrl-U` 清空、`Enter` 应用、`Esc` 取消。
3. 应用前验证两个日期和起止顺序；错误保留草稿及已应用筛选。点菜单外取消草稿，同一次点击仍生效。
4. 在 48、80、120、180 列检查选项、输入与点击区域；验证独立清除日期和页签恢复。

### Task 2: 真实历史查询

**Files:** `src/log/model.rs`、`src/log/model/tests.rs`。

1. 使用 `Option<DateFilter>` 表示无条件、最近 24 小时、最近 7 天及自定义范围，并为工具栏和页签生成可读标签。
2. 相对范围使用 `--since-as-filter=24 hours ago` / `--since-as-filter=7 days ago` 和 `--until=now`；自定义范围使用本地起日 `00:00:00` 至止日 `23:59:59`。
3. 用临时仓库和真实 Git 验证最近范围、未来时间排除、时间倒序祖先不会漏查，以及自定义范围两端的完整日期。

### Task 3: 文档与回归

**Files:** `README.md`、`docs/design.md`、`docs/prd.md`。

1. 记录菜单选项、范围输入格式、键鼠操作和日期筛选验收条件。
2. 运行 `cargo test --locked log::`；预期日期模型、界面和已有日志用例通过。
3. 运行 `cargo test --locked`、`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`，记录实际结果；不执行 commit 或 push。

### 验证记录（2026-11-04）

- `cargo test --locked date_`：6 项日期测试通过；覆盖真实 Git 时间边界及 48 / 80 / 120 / 180 列的键鼠输入。
- `cargo test --locked`：83 项单元测试、3 项集成测试通过。
- `cargo build --locked --bins` 通过；在临时 Log 演练仓使用 120×36、80×24 真实伪终端检查范围应用、再次编辑、两个快捷时段及退出。两仓 HEAD、index 条目和工作区状态均未变化。
- 最后一次全局格式和 Clippy 检查尚未通过：工作区并行新增的视图选项实现仍有格式差异，`src/log/display.rs` 的 `menu_row` 触发 `too_many_arguments`。日期实现保留这些并行改动。
