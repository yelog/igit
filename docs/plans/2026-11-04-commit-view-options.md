# Commit View Options Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 Commit 增加 IDEA 风格的显示菜单及明确的文件夹图标，保持勾选、仓库范围和 Git 内容不变。

**Architecture:** 显示状态与菜单事件放在 `src/commit/display.rs`，目录与仓库分组在现有树模型中组合。忽略项通过只读 Git 查询获取，单独显示为不可选的 Ignored Files；提交和 AI 始终排除忽略项。菜单沿用现有色板、图标回退和鼠标字符格命中。

**Tech Stack:** Rust、Ratatui、Crossterm、系统 Git；不新增依赖。

---

### Task 1: 定义显示状态与分组

**Files:** Create `src/commit/display.rs`; modify `src/commit.rs`, `src/commit/tree.rs`; test `src/commit/tree.rs`.

1. 使用 `Options { directories: false, modules: true, ignored: false, status: false }` 保留默认布局。
2. 为 Directory / Module 的四种组合增加真实多仓测试，断言叶节点身份、选区、Roots 过滤和折叠后的定位。
3. Module 关闭时平铺所有仓库；Directory 同时开启时按工作区相对路径组成目录树。
4. 运行 `cargo test --locked commit::tree::`，预期所有分组组合通过。

### Task 2: 忽略项只读展示

**Files:** Modify `src/commit/backend.rs`, `src/commit.rs`, `src/commit/tree.rs`, `src/commit/rows.rs`, `src/commit/view.rs`.

1. 运行 `git ls-files --others --ignored --exclude-standard --directory -z` 获取原生路径，忽略目录保留为单项。
2. 增加 `Change::ignored()` 与 `Change::selectable()`；所有勾选、提交、AI、移组路径排除忽略项。
3. 查询成功后才显示 Ignored Files；关闭时移除只读项并保留跟踪／未跟踪文件的勾选及撤销记录。
4. 测试 `.gitignore` / `.git/info/exclude`、忽略目录、过期勾选与无内容读取的只读预览。

### Task 3: 菜单、图标及交互

**Files:** Modify `src/commit/display.rs`, `src/commit/rows.rs`, `src/commit/view.rs`, `src/commit/keymap.rs`, `src/shell.rs`; test `src/commit/view_tests.rs`.

1. 工具栏提供眼睛下拉入口，`o` 开关；按 Group By / Show 列出 Directory、Module、Ignored Files、Git Status。
2. `j/k`、方向键、滚轮移动，Space / Enter 即时切换，Esc 关闭；菜单边框拦截下层操作，外部点击关闭并继续处理该次点击。
3. 普通仓库和目录使用展开／收起文件夹图标；忽略目录使用文件夹图标。图标空间不足时仍留标记和可读名称。
4. 在 44×12、80×24、120×36 和三种图标模式验证菜单布局与鼠标命中；保留 `t/s/v` 快捷键。

### Task 4: 文档与回归

**Files:** Modify `README.md`, `docs/design.md`, `docs/prd.md`, `docs/architecture.md`.

1. 文档记录 Module 对应仓库、默认选项、会话范围、忽略项只读与新增菜单键位。
2. 运行 `cargo test --locked commit::`；必要时运行全量 `cargo test --locked` 检查 Shell 帮助和入口。
3. 运行 `cargo fmt --all --check` 与 `cargo clippy --locked --all-targets -- -D warnings`。
4. 使用临时仓库检查混合暂存文件、未选暂存文件与忽略项，比较实际提交树、真实 index 和工作区。仅修改文件，不执行开发仓库的 commit / push。

**回归发现：** 原有按路径深度生成的 Git 排除模式可能误排除所选叶节点。索引查询改为按原生路径精确过滤，其他操作使用转义路径及必须包含子项的排除模式；回归同时覆盖中文、通配符、反斜杠、子模块更新与删除，以及未勾选后代的隔离。

**验证记录：** 显示功能的定向回归、前一轮全量测试（146 项库测试、3 项集成测试）、fmt 与 Clippy 已通过；暂存隔离另连续运行 20 次通过。最后的路径规则收尾用系统 Git 实测了嵌套特殊文件名、所选暂存版本、未选暂存内容、工作区后续修改，以及子模块指针更新和删除，均符合范围约定。收尾后的 Rust 全量复验未完成：本机 Rust 工具进程停在 `_dyld_start`，未进入程序；已取消本次挂起的验证进程，未修改系统设置。
