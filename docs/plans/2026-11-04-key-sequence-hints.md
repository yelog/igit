# 组合快捷键前缀提示 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 输入现有组合快捷键的前缀后，在底部显示当前上下文可用的后续键及说明，并支持选择、执行和取消。

**Architecture:** Log 与 Merge 根据各自正在等待的前缀提供候选，Shell 负责统一的底部浮层与事件优先级。候选选择只保存光标和绘制几何，执行时将后续键交回原面板事件路径；不新增快捷键、不改 Git 写操作、不引入依赖。

**Tech Stack:** Rust、Ratatui / TestBackend、Crossterm、临时 Git 演练仓。

---

### Task 1: 事件与渲染回归

**Files:** 新建 `src/shell/key_sequence_tests.rs`；修改 `src/shell.rs`。

1. 用临时仓库验证 `g` 不提前跳转、底部显示 `gg` 的后续键，`y` 列出 `yy/ym/ya`，补全后收起。
2. 验证方向键只移动候选，Enter 与直接补全走同一动作；Esc/Ctrl-C 只取消前缀。
3. 验证切屏、焦点变化、菜单、输入、粘贴、鼠标与窗口变化取消前缀；浮层范围拦截底层鼠标命中。
4. 在 44×12、80×24、120×36 验证候选完整、中文按字符格裁切、模式栏不被覆盖；Merge 只在打开冲突文件后提示 `[c` 和 `]c`。
5. 运行 `cargo test --locked key_sequence`，确认当前实现缺少前缀面板与交互。

### Task 2: 共享浮层与面板接入

**Files:** 新建 `src/ui/key_sequence.rs`；修改 `src/ui.rs`、`src/shell.rs`、`src/log/navigation.rs`、`src/merge.rs`。

1. 声明四种现有前缀及后续键说明；候选只从面板现有前缀状态派生。
2. 复用现有色板绘制底部边框、蓝色后续键、选中背景；按字符格宽度使用一至三列。
3. 在 Shell 普通全局键之前处理候选导航、Enter、Esc/Ctrl-C 和浮层鼠标事件；其他键保留现有语义。
4. 将候选执行交回当前面板，统一在切屏、打开全局浮层等路径清除 Log/Merge 前缀，修复 Merge 的失效前缀残留。
5. 运行 `cargo test --locked key_sequence` 与相关 Log 导航测试。

### Task 3: 文档与完整验证

**Files:** 修改 `README.md`、`docs/design.md`、`docs/prd.md`、`docs/architecture.md`。

1. 记录触发范围、候选导航、取消、鼠标和窄屏行为，增加可复现验收用例。
2. 运行 `cargo test --locked`、`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`。
3. 在临时演练仓做终端冒烟，确认浏览与取消不改变 HEAD、真实 index 或工作区。保留工作区内已有改动，不执行 commit 或 push。

### Follow-up: 200ms 延迟显示

**Files:** `src/ui/key_sequence.rs`、`src/shell.rs`、`src/shell/key_sequence_tests.rs` 及对应使用文档。

1. 在前缀候选状态中保存显示期限；期限参与 Shell 轮询等待，到点自动重绘，不延迟组合键执行。
2. 完成、取消或离开上下文时清除计时，新的前缀重新计时；未显示的候选不拦截方向键等操作。
3. 用可控的 `Instant` 验证 199/200ms 边界、无输入唤醒、快速补全、取消、新前缀及重复 Merge 前缀，不依赖测试线程休眠。
4. 运行定向与全量测试、fmt、Clippy；在伪终端检查真实延迟和快速补全不闪出。
