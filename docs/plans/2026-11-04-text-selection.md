# 文本选择与复制 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Log 详情和 Diff、Commit 消息编辑器和预览、Push Details / Diff 均可选择并复制原始文本。

**Architecture:** 增加共享文本布局与选区组件，以原始文本字节位置保存选区，将终端字符格、软换行和滚动映射回同一份原文。面板只负责区域焦点与生命周期，复制沿用 `Action::Copy` 和系统剪贴板；Shell 在存在文本选区时优先将 Ctrl-C 交给面板。编辑器复用选区，支持鼠标、Shift 导航、Ctrl-A 与选区替换。

**Tech Stack:** Rust、Ratatui 0.29 / TestBackend、Crossterm、现有 Unicode 字符格能力和剪贴板实现；不增加依赖。

---

### Task 1: 共用布局与选区

**Files:** `src/ui/selection.rs`、`src/ui.rs`。

1. 覆盖正反向、多行、空行、中文／组合字符、水平与垂直滚动、软换行、边界拖动及原文变更失效。
2. 实现共享布局投影、选区高亮、鼠标命中、复制动作；软换行不写入复制结果，保留原始缩进和换行。
3. 运行 `cargo test --locked ui::selection::`。

### Task 2: 面板接入与编辑器

**Files:** `src/log.rs`、`src/log/inspector.rs`、`src/log/preview.rs`、`src/commit.rs`、`src/commit/view.rs`、`src/push.rs`、`src/push/view.rs`、`src/shell.rs`、对应 view tests 与 keymap。

1. Log 详情、Log Diff、Commit Diff／摘要、Push 详情统一接入布局与选择；拖动文本不触发链接、树拖放或分隔线调整。
2. 鼠标左拖选择，松开即复制一次并清除选区；单击、空选区与重复释放不复制，编辑光标保留在拖选终点。键盘选区保留 Ctrl-C / Ctrl-Insert 及只读区域的 y；Esc 先清键盘选区，再按原逻辑关闭。
3. Commit 消息支持 Shift+方向键／Home／End、Ctrl-A、复制与输入／粘贴／删除替换选区；保存和取消仍沿用原行为。
4. 切换对象、打开遮挡弹窗及隐藏区域时清除过期命中与选区，滚动同一文本保留选区；Shell 的 Ctrl-C 无选区时仍走原退出行为。
5. 在 120×36、80×24、44×12 用 TestBackend 验证事件、可见高亮、消息草稿和 Git 只读性；测试不访问系统剪贴板。

### Task 3: 文档和回归

**Files:** `README.md`、`docs/design.md`、`docs/prd.md`、`docs/architecture.md`。

1. 说明选择、复制、取消、编辑替换及终端快捷键约束，并加入验收案例。
2. 运行 `cargo test --locked`、`cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`。
3. 保留工作区原有改动，不执行 commit、push 或仓库历史操作。

## 验证记录（2026-11-04）

- `cargo test --locked`：169 个单元测试及 3 个集成测试通过；新增行为另经 `cargo test --locked text_selection --lib` 和共享选区测试验证。
- `cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo build --locked --bins`、`git diff --check` 通过。
- TestBackend 覆盖 120×36、80×24、44×12；真实 120×36 PTY 通过 SGR 鼠标拖选、Ctrl-C、Ctrl-A、Shift+Left 及五处区域的复制，高亮 ANSI 输出与正常退出均已核对。
- 剪贴板使用线程局部／临时命令接收器核对字节，不改真实剪贴板；临时演练仓的 HEAD、index、工作区及操作范围保持不变。

### 后续交互调整：松开即复制

- 按用户补充要求，五处文本区域统一在左键释放时返回复制动作并清除选区，保留编辑光标终点；普通单击、空选区和重复释放不复制，父提交拖选不跳转。
- 完整回归通过 172 个单元测试与 3 个集成测试；fmt、Clippy、构建及 diff 检查通过。
- 真实 120×36 PTY 已验证：鼠标拖选期间不复制，松开后无需复制键即可将准确文本写入临时剪贴板命令；紧接着输入仍在拖选终点，Git 状态未变。

### 修复复制后单击误复制

- 已复现：旧选区已折叠清空，但旧实现将鼠标按下直接视为拖动，释放时再次映射坐标。“已复制”通知被下一次按下清除后，编辑器重排；同一屏幕坐标在 Down／Up 对应不同原文位置，从而凭空产生新选区。
- 共享组件分别记录按下坐标和实际拖动；仅收到跨字符格的 Drag 后才允许释放复制。普通单击及同字符格移动保持按下时的光标，不重新生成选区。
- 新增共享组件与 Shell 的 `click_after_copy` 回归；修复前两项均失败，Shell 在 120×35 中从预期一次复制变成两次。修复后 120×35、80×25、44×13 的重排场景、视口滚动、释放坐标抖动与原有五处区域复制测试均通过。
- 完整回归通过 176 个单元测试与 3 个集成测试；fmt、Clippy、构建及 diff 检查通过。真实 120×35 PTY 已复验“拖选复制 → 通知出现 → 再次单击触发重排 → 继续编辑”，临时剪贴板接收文件的内容与修改时间均保持不变，后续编辑、正常复制及 Git 状态核对通过。
