# Log Checkbox Style Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 Log 的 Show、Highlight 和 Columns 菜单复选框统一为 Commit 的蓝色勾选、灰色未勾选样式。

**Architecture:** 复用 `src/ui/idea.rs` 的 `checkbox`，让 `src/log/display.rs` 的菜单行分别绘制复选框和文字。行背景与禁用文字保留现有样式，Columns 入口保留等宽留白；文本截断按剩余终端格宽计算。

**Tech Stack:** Rust、Ratatui、Crossterm、现有菜单交互测试、本地临时 Git 工作区和伪终端。

---

### Task 1: 修复菜单行的样式组合

**Files:** Modify `src/log/display.rs:345`；复用 `src/ui/idea.rs:31`。

1. 在修改前确认 `menu_row` 将字形拼入整行字符串，导致复选框继承文字颜色。
2. 引入共享 `checkbox`。Columns 入口使用等宽空白 `Span`，其余行使用共享复选框 `Span`。
3. 按复选框和两格间距扣减可用宽度，单独截断文字；行背景和命中区域沿用现有逻辑。

核心绘制方式：

```rust
let mark = if matches!(hit, MenuHit::Item(Item::Columns)) {
    Span::raw(" ".repeat(cells(self.icons.check(false)).into()))
} else {
    checkbox(self.icons, checked)
};
let width = usize::from(r.width).saturating_sub(mark.width() + 2);
let line = Line::from(vec![
    Span::raw(" "),
    mark,
    Span::raw(format!(" {}", clipped(label, width))),
]);
f.render_widget(Paragraph::new(line).style(style), r);
```

### Task 2: 验证实际渲染与交互

1. 运行 `cargo test --locked view_options`，覆盖三种图标模式、主菜单与 Columns、短窗口、悬停、鼠标、键盘和列调序。
2. 运行 `cargo fmt --all --check` 和 `cargo build --locked --bin igit`。
3. 在临时演练仓通过伪终端打开主菜单和 Columns，读取实际 ANSI 颜色，核对勾选蓝色加粗、未勾选灰色、文字颜色、选中行背景与禁用项。检查前后仓库的 HEAD、index 和工作区状态。

### Task 3: 更新交互基线

**Files:** Modify `docs/design.md` 的 Log View Options 说明。

记录菜单复选框沿用 Commit 配色、行高亮与勾选颜色独立。保留当前工作区已有改动，不执行 commit 或 push。

### 完成与验证

- `cargo test --locked view_options`：10 项通过；`cargo fmt --all --check`、`cargo build --locked --bin igit` 通过。
- 伪终端验证 Nerd Font、Unicode、ASCII 三种模式，各覆盖 80×24 和 120×36。六组运行共核对 60 行状态，勾选／未勾选、浏览高亮、禁用文字、主菜单和 Columns 均符合预期。
- 两个临时演练仓的 HEAD、暂存条目和工作区状态在验证前后一致。
