# IGit 交互设计：四个面板与 Commit 改进基线

**基线：0.1.0 原生 TUI。** 四屏说明以原生客户端为准；Commit 一节对照 IDEA 的变更清单，末节记录复验方法。启动及依赖见 [README](../README.md)。原始[交互原型](reference/IGit.html)、[设计稿](reference/IGit-Design.html)和[四屏概览](reference/IGit-Overview.png)可离线查看；原型中的按钮和键位不等于已实现能力。

## 工作区与焦点

从工作目录运行 `igit` 或 `ig`。四个面板共用首行右侧的导航，顺序固定，当前面板高亮。左侧留给当前面板，Log 在这里显示日志页签和 `+`。以下以 Push 为例：

```text
 IGit   workspace / workspace                 Log  Commit  Push  Merge  Roots N/M  ⋯  ?
 ──────────────────────────────────────────────────────────────────────────── Console
 ─────────────────────────────── 当前面板 ──────────────────────────────────────────
 面板键位提示 / 最近一次操作结果
 NORMAL / INPUT / DIALOG     当前面板 · F5 refresh · ? keys                     q Exit
```

- **四种选择不要混用。** 高亮行决定正在看什么；复选框决定本次准备操作什么；Roots 限定可以操作哪些仓库；写操作的审查框决定是否执行。切换高亮行不应偷偷勾选它。Commit/Push 过滤掉的已选项保留，但不参与此次操作。工作区中至少有一个仓库时，不能应用空 Roots。
- 鼠标点右上角页签切面板，点 Roots 或按 `r` 打开筛选；用 `j/k` 移动、`Space` 切换、`a` 全选、`Enter` 应用、`Esc` 取消。空间不足时，Roots、⋯、? 移到第二行，面板页签仍在首行右侧。导航模式用 `1–4` 切换面板，`?` 查看本面板帮助，`:` 或 ⋯ 打开动作列表；`F5` 刷新当前面板，重新发现仓库用 Actions → Open / rescan workspace。
- 面板决定各区焦点与输入规则。一般用 `Tab/F6` 切区域；不要假设 Tab 会依次访问所有按钮。弹出编辑器后全局数字和字母快捷键暂停；Shell 审查框优先于面板接收事件，默认选 **Cancel**，`y` 明确确认，`n/Esc` 取消，正文较长可滚动。
- 推荐 **120×36 字符格**；低于 44 列或 12 行，Shell 提示窗口过小。四个面板及 Roots 弹窗沿用 Commit 的[共享色板](../src/ui/idea.rs)：深灰背景 `#1C1D22`、正文 `#CFD2DB`、次要文字 `#8B8F9A`、蓝色焦点 `#74A4F8`、选中行背景 `#314675`。蓝底表示当前浏览行，是否勾选看复选框。Push 与 Commit 的补丁都用绿／红区分增删；Merge 用琥珀色警告标记表示未解决冲突、绿色完成标记表示已解决块。Log 和 Commit 只在有通知时追加底部消息，Push 和 Merge 保留底部键位提示与模式栏。

**图标与字体。** 四屏及 Roots 用同一套仓库、子模块、文件和勾选标记。默认 `nerd-font` 使用具名字形；`--icons unicode` 使用标准符号，`--icons ascii` 使用文字标记。图标没有固定宽度：例如 ASCII 的 `[x]` 占三格，整枚框都是点击目标；文件标记放不下时优先留文件名。输入光标、当前行和可提交状态各有独立含义。示意图中的具体字形随模式变化。

## 1 · Log：对照 IDEA Git 工具窗口

[实际终端渲染 · 180×42](reference/IGit-Log-Graph.png) 使用[临时历史工作区](../examples/demo-log.sh)，选中了 `User: me` 和 `Paths: atlas-web`。筛选值、作者、文件和引用均来自 Git。图中是已有布局的参考画面。当前文件标记随图标模式变化；ASCII 图用 `*` / `@` 标普通提交 / HEAD，用 `|`、`-` 等字符连线。

| 旧画面的问题 | 当前的处理 |
| --- | --- |
| 每仓重复一套 Local/Remote；一长串 `origin/feat/…` 难以扫读 | 左栏固定 HEAD / Local / Remote / Tags。相同完整引用合为一行，只有部分 Roots 有它时标注仓库名；远端和斜杠前缀可折叠。当前分支钉在前面，跟踪上下游的领先／落后数来自 Git。 |
| 大标题框、表头和底部仓库图例占行，长引用把提交标题推到屏幕外 | 宽屏左／中／右约 **17 / 57 / 26%**，右侧文件／详情约 **65 / 35%**。只画单分隔线，筛选值紧贴工具栏；每条提交占一行，先读作者、时间、图、标题，引用右对齐。 |
| 仓库和分支线同色，看不出合并从何而来 | 左侧色条标仓库，金色主线和彩色分叉表示拓扑。淡蓝底是该仓 HEAD 可达的历史，选中行用更亮的蓝色；合并提交的正文降亮度。不同仓的同一 SHA 永不连线。 |
| 连续提交的点偏在线旁，或滚动时竖线错列 | 整屏共用轨道坐标。普通提交的实心点和 HEAD 的环形点都在本轨中心；兼容终端把圆点和实际相接的线画在同一格。只有分叉、汇合或换轨才有横线。太宽时，边缘的 `‹/›` 表示还有轨道，`◀/▶` 表示该行的提交在屏外。选中屏外提交会平移整幅图。 |
| 文件夹平铺，详情先露出哈希、标题靠后 | 右上是仓库 → 模块 → 紧凑目录 → 文件，容器显示文件数；右下先是完整标题，然后是 SHA、作者邮箱、本地日期与分钟、包含该提交的分支，后续可读正文与父提交。 |

| 操作 | 鼠标 | 键盘 |
| --- | --- | --- |
| 查分支或历史 | 点左栏 Branch or tag 或中栏 Text or hash；`.*` 开关正则，`Cc` 开关大小写 | 所在栏按 `/`。正文搜索输入时 `Alt-R` / `Alt-C` 也可切换；Enter 应用，Esc 取消。 |
| 筛选 | 点 Branch、User、Date、Paths，在按钮下方选择候选；点左侧分支按钮则在图标旁打开分支菜单。点清除按钮只清除该项 | `b/a/d/p` 在对应的顶部按钮下方打开菜单，方向键选候选或输入然后回车；`x` 清筛选。User 的 `me` 按各仓 Git 作者邮箱匹配。Paths 输入仓库名筛整仓，其余输入是仓库内的字面路径。 |
| 浏览与审查 | 点提交联动右栏，双击文件打开 Diff；点 Show all 展开包含分支；父提交可点击跳转 | 提交列表 `j/k` 浏览，`gg/G` 到首／末条，`H/M/L` 到本屏顶部／中间／底部；`Ctrl-F/B` 整页、`Ctrl-D/U` 半页，`PageDown/PageUp` 同整页。`h/l` 收展或换栏，`Tab/F6` 换栏。文件区 `z/Z` 全部收起／展开，Enter 查看补丁，Esc 返回；详情区 Space 展开分支。 |
| 复制提交 | — | 提交列表 `yy` 复制完整 commit ID，`ym` 复制完整提交消息（标题与正文），`ya` 复制 `姓名 <邮箱>`；显示成功或失败提示。 |
| 调整视图 | 拖动两条竖线和右侧横线；左侧窄工具栏可收起分支树／详情，切换引用与当前分支高亮 | `B/I` 隐藏或恢复左右侧栏；`+` 追加历史。提交列表的 `H/L` 用于定位，显示开关通过工具栏与显示菜单操作。少于 82 列时用 Refs / Graph / Files / Details 切单区。 |
| 保留一组筛选 | 顶部 `+` 新开 Log，点页签切回，`×` 关闭当前 Log 页；全局 Roots 仍共用 | `Ctrl-T/W` 新建／关闭，`[/]` 切换。回到原页会恢复该页筛选与可见的已选 SHA。 |

筛选菜单打开时，点菜单外会取消尚未应用的输入，同一次点击仍然生效：点 User 就切到 User，点提交行就选中它，点 Roots 就打开仓库选择。再点原按钮只收起菜单。点菜单边框或输入区不会选中下面的提交。

**Branch** 在触发按钮下打开，主菜单依次为 `Select...`、`Favorites`（超过一项收藏时）、金色星标的分支、分隔线、`Local` 与各个 `远端/...`。仓库中存在的 main/master、远端默认分支及 HEAD 默认带星。移入或点击分组打开完整分支名列表；已收藏项显示实心星，未收藏项只在高亮时显示空心星。点击星标或按空格只改收藏，点击名称或按 Enter 才应用。方向键或 `j/k` 导航，`→` 展开，`←` / `Esc` 退一级，滚轮滚动；窄屏放不下两栏时，子菜单带返回行。

[终端实图：展开远端分支](reference/IGit-Log-Branch.png)。菜单的分组和星标点击行为对照 [IntelliJ 的 Branch 过滤器](https://github.com/JetBrains/intellij-community/blob/master/platform/vcs-log/impl/src/com/intellij/vcs/log/ui/filter/BranchFilterPopupComponent.java)。

Branch 菜单中直接输入可搜索。`Select...` 支持 `|` 或粘贴多行来组合分支、`-分支` 排除可达历史、`A..B` 查看两分支差集，也可用 `origin/feat/.*` 匹配分支名。Tab 补全，Enter 或 Ctrl-Enter 应用，Esc 放弃草稿。分支与标签重名可输入 `refs/heads/…` 或 `refs/tags/…` 区分。`Favorites` 一次应用当前可见的收藏集合；清除 Branch 只移除筛选，保留收藏。

Paths 的 **Roots** 默认勾选全局范围中的所有仓库。点仓库行或用方向键定位后按空格即可逐项切换，每次勾选立即生效并保留菜单。标题显示已选数／列出数；点标题或按 `Ctrl-A` 切换列出的整组。全部取消时本页不显示提交。底部输入会筛出匹配的仓库名；在输入框按回车，完整仓库名只选该仓，其他文本作为已勾选仓库内的字面路径（例如 `./web` 可区分同名仓库）。点 Paths 的清除按钮重置本页的仓库与路径，其他条件和全局 Roots 保持原值。

Date 菜单只显示三项，依次为 `Select...`、`Last 24 hours`、`Last 7 days`，没有文本搜索框。首次打开高亮 `Select...`，已有快捷时段时高亮当前选项；方向键或 `j/k` 移动，Enter 或点击选项生效。`Select...` 打开 From / To 两个日期字段，以 `YYYY-MM-DD` 输入，Tab / Shift-Tab 或点击切换字段，Ctrl-U 清空当前字段；Enter / Apply 应用，Esc / Cancel 取消。自定义范围包含本地起日的 00:00:00 至止日的 23:59:59，也允许首尾为同一天；无效日期、缺失日期或起日晚于止日时保留草稿并提示。取消不改变已应用条件，重新编辑会带入原范围。最近时段以查询时的当前时间为终点，刷新时重新计算；清除日期保留其他筛选。

本地分支、远端跟踪分支、标签及有效 HEAD 是默认日志入口；不会因为存在 stash 就额外列出其 index 快照。文字、哈希和引用名在已加载结果中搜索；作者、日期和路径条件交给 Git。父提交未加载或被过滤时提示，不会跨仓跳转。

按住 `j/k` 或上下键时，高亮、右栏标题和哈希随选中行移动。停下后加载该行的文件、正文和包含分支；等待期间不显示上一行的文件。此时按 Enter 仍可打开当前提交的 Diff，Esc 可以取消等待。

提交列表保持现有纵向视口；向下越过最后一条可见提交时才下滚，向上越过第一条时才上滚。`H/M/L` 只选择当前屏内的提交，不滚动；不足一屏时以实际提交计算中点和底部。整页／半页按扣除工具栏与错误提示后的行数移动视口和选中行，到历史边界截断，半页至少一行。窗口缩放、筛选和父提交跳转后保持选中项可见。单个 `g` 或 `y` 等待后续按键，其他操作、换区、点击或打开浮层取消未完成的序列；输入框中的 `g/y` 仍作为文本。复制始终针对当前提交所属仓库，不等待右栏预览加载；空列表不改动剪贴板。

[实际终端菜单 · 180×42](reference/IGit-Log-View-Options.png) 展示了眼睛菜单与 Columns 子菜单；对应的 48×12、80×24、120×36 终端演练及鼠标命中也已验证。

**眼睛菜单（View Options）。** 图工具栏最右端是眼睛与下拉箭头，空间预先保留；窄屏单区模式将入口放在 Refs / Graph / Files / Details 右端。`o` 打开或收起；`j/k`、上下键和滚轮选择，`Space/Enter` 即时切换。Columns 点击、悬停或 `l/Right` 展开，`h/Left` 或 `Esc` 返回主菜单，再按 `Esc` 关闭。主菜单分为 Show / Highlight，子菜单按 Author、Hash、Date、GPG Signature 排列。菜单优先在入口下方，子菜单放在右侧或左侧，放不下则替代主菜单；短窗口围绕当前项滚动。菜单边框与留白拦截底下的行，点外部收起并执行该次点击；点 Shell 头部可直接切面板或打开 Roots。

Show 中的 Root Names 展开仓库色标为名称，Compact References View 保留首个引用并显示剩余数量；Tag Names 只控制标签文字，关闭后仍有图标。Long Edges 关闭时，跨越超过 12 行且中途无汇入的边只保留两端与跳转箭头，开启后完整连接；共享轨道保持连续，不会补画已被过滤的父子。Commit Timestamp 在作者时间与提交时间之间切换，不改变拓扑与排序。References on the Left 将引用移到标题前。四个元数据列可独立开关，列宽不足时先保留提交标题和图。

Highlight 中 My Commits 按每仓的 Git 作者邮箱匹配并加粗；Merge Commits 降低合并正文亮度；Current Branch 用蓝底表示 HEAD 可达历史，选中行始终更亮。Not Cherry-Picked Commits 只在 Branch 选了另一条本地／远端分支后可启用，按 `git cherry` 的等价补丁降低已应用提交亮度；不可用时菜单提示选择另一 Branch。默认开启 Author、Date 和前三种高亮，其他 Show 项及 cherry 高亮关闭；所有 Log 页共用本次会话的选项，重启恢复默认。日期默认是作者时间。签名只在启用列时检查，展示 Good、Unsigned、Untrusted、Bad、Expired、Key exp.、Revoked、Unchecked 或 Unknown；后两项不表示验证成功。实现见 [display.rs](../src/log/display.rs)。语义参考 [IDEA 官方 Log 文档](https://www.jetbrains.com/help/idea/log-tab.html)。

## 2 · Commit：IDEA 变更清单

[三种图标对照](reference/IGit-Icon-Modes.png)节选自同一[演练仓](../examples/demo-commit.sh)的终端输出：11 个跟踪变更和 8 个未跟踪文件。[110×36 平铺布局参考](reference/IGit-Commit-Flat.png)保留了改版前的字形。与[旧版常驻 Overview](reference/IGit-Commit.png)相比，默认画面把整块宽度留给清单。

```text
 ▾ ☑ ▤ Changes 11 files
   ▾ ☑ ■ moss-web 11 files  ├ master
       ☑ TS app.ts       moss-web/src/store/modules
       ☑ V  index.vue    moss-web/src/layouts/default/customer-service
       ☑ V  index.vue    moss-web/src/views/system/ui-config
 ▾ ☐ ⊕ Unversioned Files 8 files
   ▾ ☐ ■ moss-web 8 files  ├ master
       ☐ TS aihub.ts     moss-web/src/layouts/default/customer-service
```

两组默认展开。每仓按文件名忽略大小写排序，同名再按父目录区分；修改用蓝色、新增用绿色、未跟踪用红色。仓库图标沿用仓库色标，分支名放在灰色徽标里。只有首次出现且可提交的 Changes 文件默认勾选。蓝色行是浏览位置，复选框是提交候选；灰色父路径和标记不改变文件身份。Log 和 Commit 对同一文件使用相同的类型标记与颜色；行上的 Git 状态单独显示。

| 操作 | 鼠标 | 键盘 |
| --- | --- | --- |
| 浏览与勾选 | 点行名浏览、箭头收展、复选框勾选；滚轮移动；工具栏悬停显示动作和键位。各自有独立命中区域 | `j/k` 浏览，`PgUp/PgDn` 翻页，`h/l` 收展或去父/子行，`Space` 勾选；`u` 只撤销上次勾选 |
| 看补丁或范围 | 双击文件；工具栏的预览按钮在当前行打开预览。宽屏可继续点击左侧其他行；点关闭或返回清单按钮 | 文件上 `Enter` 看 Diff，`v` 开关预览；分组、仓库、目录预览其可见范围。92 列以上并排，更窄时预览占满工具窗。`Esc` 返回原行，`Tab/F6` 切换可用区域 |
| 切布局或看原始状态 | 目录树和状态按钮分别切换视图、显示 XY，末尾两个按钮全部展开或收起 | `t` 切目录，`s` 切 Git 双列状态，`z/Z` 全收/展开。文件身份和勾选不随布局变化 |
| 管理分组 | 添加按钮建组；点移动按钮、用右键，或把跟踪文件拖到目标组 | `n/m` 新建组／移动到组。未跟踪文件保持单列；移组不执行 git add |
| 写说明、提交 | 点底部仓名切换草稿、点消息区编辑。Amend 针对当前仓；Commit… 与 Commit & Push… 都先审查 | `[/]` 换仓、`i` 编辑、`F2/Ctrl-S` 保存、`Esc` 取消；`a` Amend，`c/P` 审查提交／提交后转 Push。AI 候选与设置是 `A/S` |

**提交对象是实际补丁。** 文件若有已暂存内容，预览及提交都以 index 版本为准；同一文件之后的工作区改动保留。按文件取舍，不支持行级拆分。AI 先展示要发送的逐仓范围，确认后才请求；收到候选后还需确认才替换原稿。实现：[commit.rs](../src/commit.rs)、[commit/backend.rs](../src/commit/backend.rs)。

## 3 · Push：核对来源、目的地与范围

```text
 Push Commits / roots in scope
 Outgoing commits                         │ Changed files · 3
 ▸ [ ] api   main → origin:main ↑1        │ Details  Expand  Collapse
 ▾ [x] web   main → origin:main ↑2        │ ▾ web 3 files
       └ 说明                             │   ▾ web 3 files
                                          │     ▾ src 2 files
                                          │         App.vue M
                                          │         main.ts A
                                          │       README.md M
 Follow tags  Hooks  Target  Fetch  Retry               Push N
```

左右各约 **50%**。左侧按仓名排序，仓名、源分支、箭头、目标、待推数量和 submodule 分列左对齐；成功／失败标记不移动分支列。可推目标使用蓝色，无待推提交的仓库行置灰。窄屏先隐藏辅助列，长文本仅在自己的列中截断，仍能通过 Target 和详情查看完整目标。

右侧默认全高显示“仓库 → 内容根 → 目录 → 文件”，连续的单子目录压缩为 `dev/iframe-proxy`，目录后显示后代文件数。文件图标沿用 Log / Commit 的类型色；文件名按变更类型着色：新增／复制绿、修改蓝、删除红、重命名／类型变化琥珀，并保留 `A/M/D/R/C/T` 文字状态。只有文件行打开该文件的 Diff；目录／仓库详情是范围摘要。打开详情后右侧约 60/40% 分割，关闭回到同一树行。

仓库行预览整批，提交行预览该次提交；合并提交的文件和 Diff 都相对第一父提交。推送单位始终是仓库的目标分支，不能仅推中间一条提交。每仓展开最多显示最近 60 条，计数和执行基于完整分支。

| 想做什么 | 鼠标 | Vim / 键盘 |
| --- | --- | --- |
| 选择和检查 | 点框勾仓、点三角收展、点行换右侧内容；点文件看差异 | `j/k` 浏览、`h/l` 或左右箭头收展，文件树中 `h` 可回父目录；`Space` 勾所属仓，`Enter` 收展容器／打开文件，`Tab/F6` 换区 |
| 文件树与详情 | 工具栏 Expand / Collapse 全部收展；Details 开关摘要／Diff | 文件区 `[/]` 全部收起／展开；`d` 开关详情、`Esc` 关闭并回文件树 |
| 核对目标与选项 | 点蓝色目标或底部 Target 编辑目标，Follow tags / Hooks 切选项，Fetch 获取范围内的远端状态 | `t` 编辑 remote:branch，Enter 保存/Esc 取消；`v/x` 切 Follow tags/hooks，`f` fetch |
| 执行并处理失败 | Push N → 核对每仓源 SHA 与目标 → Confirm；查看失败行详情，点 Retry | `p` 审查，`y` 确认；`R` 仅重试当前范围内失败且仍有待推内容的仓 |

首次直接进入选一个有待推内容的仓；从提交并推送进入时按刚成功的仓库设置选区。成功的仓库不会因为别仓失败而回滚。远端 tip 变化、非快进、无法验证修改后的子模块指针时显示原因，不把拒绝当成功。实现：[push.rs](../src/push.rs)、[push/submodules.rs](../src/push/submodules.rs)。

## 4 · Merge：应用文件和继续仓库是两步

```text
 冲突列表（仓库 + 文件） → 打开文件
 仓库 / 路径   操作类型   剩余冲突数
 ‹ Files   ↑ [c   ↓ ]c   Apply non-conflicting [a]   Undo [u]   Zoom [z]
 Current（只读）   » │ Result（可编辑） │ «    Incoming（只读）
                     │     共同基线     │
     同一个变更块    │     当前结果     │      同一个变更块
 块操作：取左 / 取右 / 取两侧 / 编辑       草稿 / Apply file
```

| 想做什么 | 鼠标 | Vim / 键盘 |
| --- | --- | --- |
| 找文件和冲突块 | 双击列表文件或点 Open；顶栏箭头跳冲突，三栏共同垂直滚动 | 列表 `j/k`、`Enter`；正文 `[c` / `]c` 为依次按两键，`j/k` 换块，`h/l` 换栏 |
| 处理变更 | Apply non-conflicting 只接收确定的部分；点左/右沟槽或块按钮，观察中栏 | `a` 应用安全块，`H/L` 取左/右，`b` 先左后右，`u` 撤销 |
| 改写结果 | 在中栏编辑当前块，点编辑区按钮接受或取消；点 Zoom 放大当前栏 | `i` 编辑、`Ctrl-S` 或 `Ctrl-Enter` 接受、`Esc` 取消；`z` 单栏，`s` 保存会话草稿 |
| 写入、继续 | Apply file → 审查并确认，此步只写入并暂存该路径。回到列表，在仓库操作上另选 Continue | 文档内 `A` 审查应用；列表中 `C` 审查继续。打开文件后 `Esc` 返回列表 |

默认三栏等宽；低于 **72 列**只显示焦点栏，工具栏的 Apply non-conflicting 简写为 Safe，上一块和下一块按钮只显示方向。左右按真实操作显示来源，rebase 下为基底与被重放提交，不固定说“本地/远端”。文件已有外部编辑或索引已变化时拒绝覆盖；二进制、删除/修改、特殊属性等未支持情形转外部处理。没有冲突时显示空态。这里处理已有 Git 操作，不提供创建 merge/rebase 的分支按钮。实现：[merge.rs](../src/merge.rs)、[merge/engine.rs](../src/merge/engine.rs)。

## Commit 对照验收

1. 运行 [Commit 演练脚本](../examples/demo-commit.sh)，用其输出的路径启动 IGit，按 `2`。在 120×36 检查 19 项全部可见；在 80×24 检查可滚到最末项。区分全选、半选、未选、不可选，以及独立的行高亮。再用 `--icons ascii` 点三格复选框的首尾，均应只切换当前项；点文件名只浏览。
2. 分别打开两个 `index.vue`，核对路径和补丁。切目录树，折叠并展开祖先；切回平铺，仍对准同一文件。容器预览只列该范围，浏览、折叠和切换布局均不改勾选或 index。
3. 只勾选 `scripts/iframe-proxy.mjs`，打开 Diff。它有已暂存和未暂存两份改动：预览和提交审查只含暂存版本。取消审查后比对 HEAD、真实索引和工作区。
4. 多仓范围用[多仓演练脚本](../examples/demo-workspace.sh)复验：隐藏一个已勾选仓，打开提交审查，确认只列可见的候选。详见 [PRD A2 / A7 / A9](prd.md)。
