# IGit：接手项目

IGit 是独立的多仓 Git 终端工作台，`igit` 和 `ig` 进入同一应用。产品以 IntelliJ IDEA 的 Git 工作流为交互参考；正式程序用 Ratatui / Crossterm 绘制并调用系统 Git。不要从浏览器原型推断某项能力已在终端实现。

## 先读什么

| 本次任务 | 先读 | 从哪里动手 |
| --- | --- | --- |
| 确认范围、优先级、验收条件 | [产品需求 PRD](docs/prd.md) | 对照“已实现”和“待验收”；新需求先确定涉及哪个仓库、哪些文件、是否写入。 |
| 调整布局、鼠标命中或 Vim 快捷键 | [交互设计](docs/design.md) · [工作区实图](docs/reference/IGit-Split-Workspace.png) | 分屏与 Push/Merge 弹窗的几何和鼠标路由在 [workspace.rs](src/shell/workspace.rs)；面板内部看各自的 `draw`、`event`，全局快捷键在 [shell.rs](src/shell.rs)。 |
| 修改 Git 命令、状态或跨仓操作 | [架构](docs/architecture.md) | [core.rs](src/core.rs) 负责仓库发现和共享 Git 调用；事务细节在各面板的实现里。 |
| 启动、体验操作或配置 AI | [README](README.md) | 使用本地演练工作区；无需连接外部仓库或配置模型。 |

**当前交接点（2026-11-04）：** Commit 默认对齐 IDEA 的平铺变更清单：分组 → 仓库 → 文件，文件名后标父目录；未跟踪文件展开但不自动勾选。`t` 切目录树，`v` 打开预览（文件 → Diff，容器 → 范围摘要）。从[树模型](src/commit/tree.rs)、[单行绘制与命中](src/commit/rows.rs)、[视图](src/commit/view.rs)和[事件](src/commit.rs)定位修改。用 PRD 的 A7 检查跨仓范围，用 A9 和[演练仓](examples/demo-commit.sh)检查同名文件、窄屏与暂存内容。

## 开发时守住的行为

- 高亮行是预览对象；复选框决定候选项；写操作只执行当前范围内、用户审查过的项。不要为了完成批次自动加选其他仓。
- 修改 Commit 时，同时检查工作区、真实 index 和提交出的树。特别验证“一份文件既有暂存修改又有未暂存修改，另一份文件已暂存但没被选中”。
- 多仓提交/推送分别产生结果，没有跨仓原子性。失败重试不重复已成功的项；父库推送不能发布无法获取的子模块指针。
- 合并中的“应用文件”与“继续仓库操作”需要分别审查；出现外部改动或不支持的冲突类型时保留现场。
- AI 消息是可编辑候选。只在用户审查后向已配置的端点发送勾选的补丁，密钥不入仓；日常回归不使用真实模型或用户仓库。

## 快速复现与验证

从仓库根目录运行。演练脚本会创建新的临时工作区、两个普通仓库、一个子模块和本地 bare 远端，打印工作区路径：

```sh
demo_workspace="$(sh examples/demo-workspace.sh)"
cargo run --locked --bin igit -- "$demo_workspace"
```

交互冒烟：按 `2` 进 Commit；分别用鼠标和 `j/k` 看文件、选分组，确认浏览不改变勾选；给两个已勾选的仓库分别写好说明，再按 `r` 隐藏一个仓、`c` 打开审查，检查只包含可见的已选项，最后按 `Esc` 取消。在改动涉及写操作时，去演练仓里用 `git status`、`git diff --cached`、`git log` 或远端引用核对结果。

按改动选择检查：`cargo test --locked commit::` 是 Commit 的定向测试，跨面板或共享 Git 逻辑改动运行 `cargo test --locked`。提交代码前常用 `cargo fmt --all --check` 和 `cargo clippy --locked --all-targets -- -D warnings`。纯文档改动核对相对链接、命令和实现声明即可。

修改产品范围或已交付状态时更新 PRD；更改焦点、键位和面板行为时更新交互设计和 README；变更模块职责、持久化或 Git 执行边界时更新架构。历史原型保留为设计基线，不作为运行时依赖。
