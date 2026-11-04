# IGit 架构：从一次操作追到真实 Git

IGit 是一个 Rust 终端程序。两个入口 [igit](../src/bin/igit.rs) 和 [ig](../src/bin/ig.rs) 都调用 [igit::run](../src/lib.rs)；Crossterm 读取输入，Ratatui 绘制界面，系统 `git` 负责读写仓库。Git 的工作区、索引、对象和引用是事实来源；界面保存的是用户的选择、草稿和对某一时刻仓库状态的预览。

## 从哪里读代码

| 要回答的问题 | 入口及职责 |
|---|---|
| 当前目录里有哪些仓库？Git 命令在哪个目录执行？ | [core.rs](../src/core.rs)：`Context`、`Repo`、发现、作用域和 Git 子进程环境。 |
| 谁处理快捷键、鼠标、弹窗和跨面板跳转？ | [shell.rs](../src/shell.rs)：`Shell`、事件循环、Roots 和确认框；[ui.rs](../src/ui.rs)：`Panel`、`Action` 和共用样式。 |
| 同一文件或按钮为什么显示这个图标？ | [ui/icons.rs](../src/ui/icons.rs) 按语义、文件名和扩展名映射字形。Shell 将启动时的 `IconMode` 传给四个面板；布局和命中读取实际终端列宽。图标偏好不写入仓库、草稿或 Git 配置。提交图的轨道和节点由 [graph/render.rs](../src/log/model/graph/render.rs) 按拓扑绘制。 |
| 为什么这条提交出现在该位置？ | [log/model.rs](../src/log/model.rs) 读真实 Git 引用、提交、可达性与补丁；[graph.rs](../src/log/model/graph.rs) 分配轨道。[log.rs](../src/log.rs) 联动选择和事件；[view.rs](../src/log/view.rs)、[refs.rs](../src/log/refs.rs)、[inspector.rs](../src/log/inspector.rs) 分别绘制时间线、分支树、右栏；[filters.rs](../src/log/filters.rs) 管筛选与会话内页签。 |
| 这次 commit 包含哪些内容？ | [commit.rs](../src/commit.rs)：分组、勾选、逐仓说明、AI 与批量提交；[commit/tree.rs](../src/commit/tree.rs)：平铺／紧凑目录两种投影和原生文件身份；[commit/rows.rs](../src/commit/rows.rs)：单行绘制及箭头、复选框的共用坐标；[commit/view.rs](../src/commit/view.rs)：变更列表、消息栏与按需打开的预览；[commit/backend.rs](../src/commit/backend.rs)：status、快照、计划与提交执行。 |
| 会推送到哪里，子模块提交在目标端能否取到？ | [push.rs](../src/push.rs)：待推送提交、目标、计划、后台任务、逐仓结果；[push/files.rs](../src/push/files.rs) 解析变更类型并投影文件树，[push/view.rs](../src/push/view.rs) 绘制列与命中区；[push/submodules.rs](../src/push/submodules.rs)：待发布历史中 gitlink 的远端可达性检查。 |
| “智能合并”什么时候能自动选择？ | [merge.rs](../src/merge.rs)：冲突导航、结果编辑、对话框；[merge/engine.rs](../src/merge/engine.rs)：读取 stages、三方比较、写文件并暂存、继续 Git 操作。 |

## 一次按键或点击的路径

1. [shell::run](../src/shell.rs) 创建工作区和四个面板，开启 alternate screen、raw mode、鼠标和括号粘贴。事件循环处理输入、轮询后台结果，并在需要时重绘；退出时清理终端模式。
2. `Shell::dispatch` 优先把输入交给活动弹窗，再处理全局导航和当前面板。面板通过 `editing()` 声明正在输入文本，避免输入框中的普通字符触发全局命令。鼠标操作使用最近一帧绘制出的区域；更改布局也要检查点击区域。
3. 活动面板用 `Panel::event` 接收事件并返回 `Action`。面板内的选中行、焦点和滚动位置由面板自己维护；切换面板、显示提示、确认操作交给 Shell。需要确认时面板保存具体计划，返回 `Confirm { title, body, token }`；Shell 记录发出请求的面板，默认选中取消，确认后才回调该面板的 `confirm(token, context)`。
4. 例如点击 **Commit & Push**：Commit 构建各仓计划 → 用户审查 → 逐仓提交 → `Action::Committed` 携成功仓集合返回 → Shell 切到 Push 并将成功仓设为初始勾选 → 用户单独审查并发起推送。“已创建本地提交”和“远端已更新”是两个独立结果。

四个面板在同一 Shell 内常驻。切换面板、刷新当前面板不会重建整个 Shell；打开另一工作区或重新扫描会重建上下文和面板。Log 历史刷新和显式打开补丁、Commit 快照和执行、Merge 读写走同步调用；Log 提交详情、Push/Fetch 和 AI 请求使用标准线程与 `mpsc` 回传结果，由 Shell 轮询。当前没有通用异步任务调度器；大型仓库的同步刷新、耗时 hook 都可能阻塞输入。

## 仓库、作用域与持久化

- **发现。** `Context::discover` 先找启动目录所在的工作树，再向启动目录下方遍历。用 Git 解析含 `.git` 文件或目录的工作树并去重，忽略 bare 仓库和常见构建、依赖目录。用 `--show-superproject-working-tree` 判断 submodule；只有父工作树也被发现时才有 `Repo.parent`。普通嵌套仓库没有这种依赖。未初始化的子模块不是可操作工作树。
- **身份。** `repos: Vec<Repo>` 按规范路径排列；`scope: BTreeSet<usize>` 是当前允许显示或操作的仓库下标集合，初始包含全部。仓库 ID 只在本次发现结果内有效；重扫后不可用旧下标恢复身份。界面展示的仓库名、缩写 SHA、文件行号也都不是持久键。缩小 Roots 不能让隐藏仓悄悄参加下一次写入。
- **本地状态。** 状态目录依次选取 `$IGIT_STATE_DIR`、`$XDG_STATE_HOME/igit`、`$HOME/.local/state/igit`，再追加规范工作区路径的散列值。其下的 `commit.json` 保存分组、文件归属、逐仓消息草稿及 AI endpoint/model/prompt，通过同目录临时文件替换。内部键由仓库路径、文件路径的字节编码组成。移动工作区会产生新的状态目录；同时打开两个实例没有草稿合并机制。勾选、焦点、待执行计划、推送结果和合并撤销栈不在这个文件中。
- **命令环境。** 从 `core::command` 派生的 Git 进程使用 `git -C <repo>`，清除继承的工作树、索引和对象目录等覆盖变量，禁用交互式终端提示。私有索引由调用方显式注入。Git 已配置的身份、hook、签名及凭据助手仍会影响结果；交互式凭据输入不是 TUI 的一部分。解析路径时使用 NUL 分隔的字节流；显示用的有损字符串不能传回 Git 充当路径。

## 四条核心数据路径

Log 的[显示菜单](../src/log/display.rs)保存会话级偏好和临时菜单焦点，不写入项目或状态文件；过滤条件与选中 SHA 仍由各 Log 页保存。`Rev` 同时保存作者邮箱、作者日期与提交日期，排序继续使用提交时间。刷新及切页后，[呈现元数据](../src/log/model/presentation.rs)按仓读取身份；仅在签名列开启时对已加载 OID 分批读取 `%G?`，仅在 cherry 高亮开启且选了另一分支时执行 `git cherry HEAD <resolved-tip>`。查询走共享 Git 环境，失败按仓显示并保留历史；渲染、悬停、逐行导航不发这些命令。签名验证和补丁比较沿用同步刷新路径，大型历史启用这两项可能增加刷新耗时。长边缩短只影响字符绘制，箭头命中保存真实可见端点，不修改 DAG。

**Log：同屏展示多个提交图。** 每仓从本地分支／远端跟踪分支／标签和有效 HEAD 开始，按 `git log --date-order` 取拓扑有效的提交序列，再按各序列头部时间合并。[轨道布局](../src/log/model/graph.rs)按「仓库 ID + 对象 ID」匹配可见的真实父子。长竖线只保存起止行；指向同一父提交的路径可共用后段，主线优先延续第一父提交。轨道结束后可复用空列，换列处记录转折。[字符绘制](../src/log/model/graph/render.rs)只展开当前视口，所有行共用横向起点。每仓默认 160 条、上限 3,200 条。切换文本或哈希筛选不重读整批历史，正则用 `regex-lite` 在内存里匹配。按完整引用名跨仓查询，并用 Git 的实际命名空间区分同名分支、远端和标签。多个 Log 页只保存筛选与选中对象，切页时复用同一面板并重新加载可见数据；Roots 对所有页生效。

[Branch 菜单](../src/log/branches.rs) 按当前页的 Roots 从已加载引用构建候选，同一完整引用跨仓只列一次。鼠标悬停、子菜单滚动及搜索不启动 Git。工作区状态目录下的 `branch-favorites.json` 只保存手动星标偏好，用临时文件原子替换，不修改仓库配置。每个 Log 页保留自己的[分支表达式](../src/log/model/branch_filter.rs)：执行查询前逐仓匹配完整引用，日志及可达性检查使用相同的起点和排除项；只有一个明确分支时才开启 cherry 比较。

[Paths](../src/log/paths.rs) 将本页的仓库 ID 集合与字面路径分别保存。勾选仓库只从全局 Roots 已加载的提交中筛出相应行，不重复运行 `git log`；加载更多和错误提示也只看当前勾选的仓库。输入文件或目录仍由 Git 用 `--literal-pathspecs` 匹配。

提交列表在内存中移动光标。连续移动时，[详情加载](../src/log/preview.rs) 等选中行稳定 60 毫秒，再从该仓读取变更文件、正文和包含分支，并在后台构建文件树。同时最多运行一次详情查询，只保留最新待查行；切换选择或刷新会使旧请求失效，即使刷新前后的 SHA 相同也会重新读取分支。Shell 按顺序处理已经到达的连续移动键，一次重绘它们的最终位置；遇到其他操作先重绘，鼠标据此命中当前行。

[列表导航](../src/log/navigation.rs) 保存纵向起点与最近绘制的有效行数，每次选择变化就校正边缘，保证批量处理输入与逐键重绘得到相同的滚动结果。绘制时根据新窗口高度再次校正；`gg` 与 `y` 系列仅在提交列表中识别，换焦点或进入浮层时清除前缀。复制经 `Action::Copy` 交给 Shell 和[剪贴板适配](../src/ui/clipboard.rs)，通过 stdin 调用系统命令，不把文本拼进命令或记录进通知。ID、姓名和邮箱取当前行；完整消息仅在 `ym` 时对该仓的准确 OID 运行只读 `git show`，不依赖可能仍在加载的详情。

**Commit：勾选的是内容，不是整个真实索引。** 从 `git status --porcelain=v1 -z` 获得变更。默认按仓库和文件名平铺，用各自的原生路径处理选择、勾选和移组，父目录提示仅用于显示。切换目录模式和展开状态不会触碰 index。关闭预览时不读取文件补丁；打开时根据当前行决定加载文件 Diff 或显示内存中的容器摘要。审查时从 HEAD（首个提交为空树）构造临时索引：选中文件已有暂存内容则使用该版本；否则读取选中的工作区内容。例如同一文件既 staged 又有后续修改时，提交只取 staged 版本。用 `write-tree` 得到拟提交的 tree，并保留 HEAD、分支和选中项的索引记录。审查可能生成 Git 对象，但不移动 HEAD 或覆盖真实索引。

确认后，`backend::execute` 以独占创建方式获取真实的 `index.lock`，再核对分支、HEAD、索引记录和重新计算的 tree。实际 `git commit` 使用私有索引并运行原生 hook；新真实索引从锁定时的索引副本构建，保留无关文件原有的暂存内容。hook 改写提交时报告结果并要求复核；如果 HEAD 已更新而索引发布失败，错误提示会保留恢复锁。此时应先检查 HEAD、索引和报错，不能自动删锁或再次提交。

**AI 提交说明：请求按仓构建，输出仍是草稿。** Commit 用勾选项生成 patch，审查界面列出将发送的目标和范围；确认后后台调用 `curl` 请求配置的接口。API key 从 `IGIT_LLM_API_KEY` 读取，不写入 `commit.json`。结果先作为候选稿，须由用户采用和审查；生成成功不会创建提交。排查时不要把凭据或完整的私有仓库 patch 加入日志。

Commit 和 AI 共用勾选文件的快照。文件与目录互换时，用实际变更路径核对范围；若加入一个文件必须删除未勾选的子路径，停止并请用户重新选择。Commit 确认页的分支来自同一份待执行计划，分支列表的缓存不参与决定目标。

**Push：推送审查过的对象。** 每个 Job 记录仓库、源分支和 HEAD、目标 remote/branch、push URL 和预期远端 tip。执行前复查源状态，向实际 **push URL** 查询目标 tip。祖先关系用 Git 对象中记录的父提交验证，忽略本地替换引用和旧式 graft；通过后才用针对目标引用和预期值的 `--force-with-lease` 推送已审查的 OID，防止检查后远端被并发改写。待发布历史中，只要 gitlink 改变或对应路径的来源改变，就检查该提交记录的子模块版本能否从当时声明的来源取得。无法证明则拒绝父仓推送，不会隐式推送其他仓。

**Merge：预览、暂存和 Continue 各有边界。** 从索引读取 stage 1（base）、2（ours）、3（theirs）；分别计算 base→两侧的行差异，按 base 坐标组成块。仅一侧改动或两侧产生相同结果的块可一键采用，其余留给用户。stage 2/3 表示 Git 的两侧，rebase 时不宜把右侧一律称为“远程”。选择和编辑首先只改内存中的 `Document`。

Apply 确认后重新校验 operation、HEAD、冲突 stages、文件原内容、权限和转换配置；在原目录写临时文件并替换原文件，再单独 `git add -- <path>`。暂存失败且工作文件仍是刚写入的结果时，尝试恢复原内容。对非普通三阶段文本冲突、二进制、需 checkout 转换、超出大小/行数限制或已被外部编辑的冲突文件，不直接覆盖。**Continue 是逐仓操作**：另行审查并验证整个索引的 tree，运行相应 `git … --continue`；仓内其他已暂存文件也在范围内。

## 批量写入与失败

| 操作 | 遇到某仓失败时 |
|---|---|
| Commit | 按路径深度子仓优先，在 UI 线程逐仓执行。保留已成功的 SHA；如果父仓选中了失败子仓的 gitlink，跳过依赖的父提交。子仓成功后父仓指针若已偏离原预览，须重新审查；独立仓可继续。 |
| Push | 一个后台任务逐仓执行；失败会阻止对应父仓链继续推送，但不阻塞无依赖的仓。结果按仓回报。重试按当前 Roots 和失败记录重新生成审查，不重放旧远端计划。 |
| Merge | Roots 可同时汇集多个仓的冲突，Apply 针对一个文件，Continue 针对一个仓。没有跨仓“全部应用并继续”的事务。 |

这里的「同时提交／推送」表示一次可选多个仓，不保证并行或全有全无。应用内的确认框和 busy 状态只协调本实例，不能锁住外部终端、其他 IGit 实例或远端；Git 锁、快照复核和远端 lease 分别处理不同的竞争窗口。

## 修改与验收

| 改动 | 从哪里验证 |
|---|---|
| 仓库发现、重名、submodule 或工作区边界 | [tests/discovery.rs](../tests/discovery.rs)，运行 `cargo test --locked --test discovery`。 |
| 图、筛选、键盘和鼠标联动 | `cargo test --locked log::`；在临时多仓工作区检查切焦点、缩窄、滚动后的命中。 |
| Commit 选择或快照 | `cargo test --locked commit::`；既查实际 commit tree，又比较未勾选文件的暂存内容和工作区，不能只断言“命令成功”。 |
| Push 目标、远端竞争或子模块 | `cargo test --locked push::`；用临时 bare remote 检查目标 ref、部分失败和重试范围。 |
| 三方差异、Apply、Continue | `cargo test --locked merge::`；测试真实冲突 stages、写入后文件字节、暂存状态以及外部编辑后拒绝覆盖。 |

全量入口是 `cargo test --locked`；本地交互验收可按 [README](../README.md) 使用 [示例工作区脚本](../examples/demo-workspace.sh)，不要拿日常仓库试写操作。新增按钮先定位它应发出的既有动作，再把键盘、鼠标接入同一路径；新增 Git 写操作则先定义计划审查什么、执行时复核什么、部分成功如何呈现，再接入面板。同步调用若成为性能瓶颈，可参照 Push 的后台结果通道；让结果携带工作区与请求身份，避免较慢的旧请求覆盖当前选择。这是扩展建议，并非已有的全局异步机制。
