# Log 列表 Vim 导航与复制 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 提交列表在越过可视边缘时才滚动，并支持 gg、H/M/L、整页／半页翻页和 yy/ym/ya 复制。

**Architecture:** Log 保存提交列表的纵向起点与实际可用行数，在每次选择变化及绘制时校正视口。新的导航模块处理提交列表的按键序列；复制通过 Panel Action 交给 Shell 写入系统剪贴板，提交信息只在 ym 时读取完整正文，不依赖异步预览。

**Tech Stack:** Rust、Ratatui TestBackend、Crossterm、系统 Git、系统剪贴板命令；不新增依赖。

---

### Task 1: 视口与导航回归

**Files:** `src/log.rs`、`src/log/navigation.rs`、`src/log/view.rs`、`src/log/view_tests.rs`。

1. 添加渲染与事件联动测试：j/k 到边缘才滚动，批量输入保持顺序，gg 两键跳首行，G 到末行，H/M/L 只移动到当前可视提交，Ctrl-F/B/D/U 按实际行数翻页。
2. 运行 `cargo test --locked log::view_tests::navigation_`，确认新测试暴露当前行为。
3. 保存纵向视口；行选择和窗口尺寸变化时保持选中项可见。整页移动实际可用行数，半页移动其一半（至少一行），到边界截断；PageUp/PageDown 使用相同整页行为。
4. 处理 g/y 前缀，其他操作、焦点改变和输入浮层取消待完成序列。H/L 在提交列表中使用新导航含义；显示功能保留工具栏和菜单入口。

### Task 2: 复制选中提交

**Files:** `src/log/navigation.rs`、`src/log/view_tests.rs`、`src/ui.rs`、`src/ui/clipboard.rs`、`src/shell.rs`。

1. 添加 yy/ym/ya 用例，核对完整 OID、包含正文的消息和姓名／邮箱，特别覆盖刚移动到另一仓且详情尚未加载的情况。空列表不能复制；无效序列不能触发筛选或误复制。
2. 通过 `Action::Copy` 传递文本与成功提示；Shell 使用 macOS pbcopy、Windows clip 或 Linux wl-copy/xclip/xsel，将文本经 stdin 写入，失败明确提示且不记录复制内容。
3. 用临时接收命令验证剪贴板写入字节及失败处理，不改动测试运行者的真实剪贴板。

### Task 3: 文档与验证

**Files:** `README.md`、`docs/design.md`、`docs/prd.md`、`docs/architecture.md`、`src/shell.rs`。

1. 更新应用帮助、键位表、视口与复制边界，以及可复现验收条件。
2. 运行 `cargo test --locked log::`，再运行 `cargo test --locked`、`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`。
3. 只修改本任务相关文件；保留现有工作区改动，不执行 commit 或 push。
