# IGit

用 `igit` 或 `ig` 打开同一个多仓 Git 工作台。Log、Commit、Push 和 Merge 都在一个终端会话里运行。Rust + Ratatui + Crossterm 绘制界面，实际仓库操作调用系统 Git。适合在 kitty、Terminal 等支持 ANSI 和鼠标报告的终端运行；建议 120 列 × 36 行以上。

## 项目文档

- [新会话入口：AGENTS.md](AGENTS.md) — 按任务选择文档和源码，附本地演练和验证命令。
- [产品需求（PRD）](docs/prd.md) — 使用场景、已交付范围、待办和验收标准。
- [交互设计](docs/design.md) — 四个面板的布局、焦点、键鼠规则，以及 Commit 对齐 IDEA 的验收标准。
- [架构](docs/architecture.md) — 事件如何到达面板、Git 写入如何审查、跨仓失败如何处理。

第一版设计参考：[四屏总览](docs/reference/IGit-Overview.png) · [可点击原型](docs/reference/IGit.html) · [配图设计说明](docs/reference/IGit-Design.html)。两个 HTML 文件可在本地浏览器打开，使用演示数据。

## 启动

在本源码目录编译并启动；如需在任意工作区使用 `igit` 或 `ig`，可安装到 Cargo 的 bin 目录：

```sh
cargo build --release --locked
./target/release/igit /path/to/workspace
./target/release/ig /path/to/workspace

# 可选：安装两个命令到 Cargo 的 bin 目录
cargo install --locked --path .
```

`igit` 和 `ig` 默认从当前目录发现仓库。可传入目录；`--list-roots [目录]` 只列出发现的工作树。支持普通 Git 根、多层嵌套仓库、已初始化的 submodule 及 linked worktree。裸仓库不作为可编辑目标，符号链接目录和常见依赖/构建目录（如 node_modules、target、.venv、build）不递归；不会初始化或克隆子模块。若仓库位于默认跳过的目录中，可以直接以该目录启动。

**选择终端图标。** 默认使用 Nerd Font 的仓库、分支、文件类型和操作图标。若看到方框，试试 `ig --icons unicode`；想用 `[x]` 这样的文字标记，可用 `ig --icons ascii`。两个命令都接受 `--icons 模式 [目录]`，同一次启动里的四个面板和重新打开的工作区都沿用该模式。

| 模式 | 适用场景 |
| --- | --- |
| `nerd-font`（默认） | 本机终端已配置 Nerd Font 3.x 或 Symbols Nerd Font Mono。图标区分仓库、子模块、分支、标签及常见文件类型。 |
| `unicode` | 无专用字体；用标准符号和文件类型缩写，如 `☑`、`TS`。 |
| `ascii` | 用 ASCII 字符显示图标和提交图，例如 `[x]`、`[-]`、`[ ]`、`[!]`。界面文字、边框、文件名和补丁仍可包含 Unicode。 |

IGit 不探测或安装字体。通过 SSH 使用时，由本机终端的字体决定字形。

![同一演练仓的 Commit 清单：Nerd Font、Unicode 和 ASCII 图标对照](docs/reference/IGit-Icon-Modes.png)

**建议先试真实的练习工作区**（只在新的临时目录创建仓库；远端全在本机，不会推到托管服务）：

```sh
demo_workspace="$(sh examples/demo-workspace.sh)"
igit "$demo_workspace"
```

这里包含两个普通仓库、一个已初始化的子模块、三个 bare 远端。普通仓各有待提交和待推送的改动；子模块内有一处真实冲突和两处独立改动。脚本打印工作区路径；不会删除练习结果。

## 全局

| 操作 | 键盘 | 鼠标 |
| --- | --- | --- |
| 切换工作面板 | `1` Log、`2` Commit、`3` Push、`4` Merge | 点击右上角的 Log、Commit、Push、Merge；左侧的 Log 页签用于切换日志筛选视图 |
| 筛选仓库 | `r` → `j/k` → `Space` 切换；`a` 全选；`Enter` 应用 | 点 Roots、仓库行和 Apply |
| 区域与动作 | `Tab/F6` 在当前面板内换区；`:` 动作列表；`?` / `F1` 当前区域帮助 | 点击区域、⋯（Actions）、?（Help） |
| 刷新与退出 | `F5` 刷新；`q` 或普通模式 `Ctrl-C` 打开退出确认 | 点 ⋯ → Refresh panel 或 Quit |
| 审查写操作 | `y` 确认；默认焦点在取消，`n/Esc` 取消；长内容用 `j/k` 或方向键滚动 | 点 Confirm / Cancel，滚轮浏览 |

当前行决定预览，复选框决定本次操作。筛掉的仓库不参与执行。输入文本时字母与数字只作为文本，不会执行全局快捷键。Actions → Open / rescan workspace 可重新发现仓库；打开工作区将重新开始面板会话，请先结束当前输入。

**选择文本并复制：** Log 右下角提交详情及文件 Diff、Commit 的 Edit commit message 与 Diff preview、Push 的 Details / Diff 都支持左键拖动选择，拖选期间以蓝底显示；**松开左键立即复制并清除选区，无需再按快捷键**。普通单击或空选区不复制，父提交链接单击仍可跳转，拖选不会跳转。复制保留原始缩进、空行和换行，不带软折行、边框或终端填充空格。拖选过程中可滚动，切换提交／文件或隐藏区域会清除旧选区。

键盘操作仍支持 `Ctrl-A` 全选、`Ctrl-C` / `Ctrl-Insert` 复制，只读区域也可按 `y`；键盘选区随滚动保留，`Esc` 先清除选区。编辑提交说明可用 `Shift+方向键`、`Shift+Home/End` 选择要替换的文本，输入、粘贴、Backspace 和 Delete 替换或删除键盘选区；鼠标松开自动复制后，光标保留在拖选终点。滚轮浏览长消息，按键移动光标时保持光标可见；`F2/Ctrl-S` 保存，无选区时 `Esc` 取消编辑。有选区的 `Ctrl-C` 优先复制；普通模式无选区时仍打开退出确认。剪贴板沿用下方列出的系统命令，失败会明确提示。

**上下文帮助：** `?`、`F1` 或右上角 Help 打开与 lazydb 一致的 `KEYMAP // 当前区域` 面板，默认聚焦 Search，直接输入快捷键、中文说明或区域名筛选。当前区域的动作排在最前，其后为当前面板通用与全局动作；每行对应一个明确动作。`↑/↓` 选择，`Enter` 关闭帮助并执行选中快捷键；`PgUp/PgDn` 翻页选择，鼠标点击选择，滚轮或滚动条浏览。Search 支持左右光标、Home/End、删除及 `Ctrl-W/U/A/E` 编辑；`Esc/F1` 取消帮助并恢复原焦点、草稿或弹窗。输入框、Diff、Roots 和审查框中也可用 `F1`；选中提交、推送或应用文件等动作后仍进入原有审查流程。

**组合键提示：** 在 Log 提交列表按 `g` 或 `y`，或在 Merge 冲突文档按 `[` / `]`，等待 200ms 仍未按下其他键时，底部列出该前缀的后续键与说明。继续输入后续键即可执行，快速补全组合键不会闪出提示；候选显示后，也可用方向键选择后按 `Enter`，或点击候选执行。`Esc/Ctrl-C` 只取消本次前缀；切换面板、焦点、打开菜单、粘贴或调整窗口也会收起，改按另一个前缀则重新计时。面板随宽度使用一至三列，输入框中的字母和 Commit/Push 中的单键操作保留原有含义。

## 四个面板

**Log 显示选项：** 点日志区域右上角的眼睛，或按 `o`，打开 Show / Highlight 菜单。**Show → Branch or Tag** 控制左侧分支树，默认勾选；也可按 `B` 切换。隐藏后提交列表贴到左边缘，窄屏的 Refs 入口也随之隐藏。`j/k` 或方向键选择，`Space/Enter` 切换；Columns 可点击、悬停或按 `l/Right` 展开，`h/Left` 或 `Esc` 返回，主菜单再按 `Esc` 关闭。短窗口可滚动菜单，窄屏子菜单会移到左侧或替代主菜单。

**调整日志列顺序：** Columns 菜单从上到下，就是日志行从左到右的顺序。选中一列，按 `K` 或 `Shift+↑` 左移，按 `J` 或 `Shift+↓` 右移；鼠标点列名右侧的上下箭头也可以移动。点列名或按空格仍只切换显隐。隐藏列也能先排好位置，再勾选显示。

可切换仓库名、紧凑引用、标签名、长分支线、作者／提交时间、引用位置，以及 Author、Hash、Date、GPG Signature、Graph、Message 六列；My Commits（粗体）、Merge Commits（降亮度）、Current Branch（蓝底）独立控制。选择另一 Branch 后，Not Cherry-Picked Commits 用补丁等价比较降低已应用项的亮度，保留尚未应用项的正常亮度。长线关闭时，超过 12 行且中途无汇入的连接中部以可点击箭头代替，点击跳到对应端点。

默认显示左右侧栏、引用标签和 Author、Date、Graph、Message 四列，并开启前三种高亮。Graph 与 Message 也能独立隐藏、移到任意列之间；日期默认显示作者时间。关闭 Tag Names 仍显示标签图标，名称可在左侧 Tags 中查找；窄列优先保留图和提交标题。选项在本次运行的 Log 页间共享，重启恢复默认；签名与 cherry 比较只在启用时查询，不改写仓库。

**Log 分支筛选：** 点 **Branch** 或按 `b`，可直接选收藏的分支，或移入 **Local**、**origin/...** 等分组展开子菜单。点星标（键盘用空格）只切换收藏，点名称或按 Enter 筛选日志。**Favorites** 显示所有收藏分支的提交；**Select...** 支持用 `|` 同时指定多条分支，例如 `master | origin/release`。

提交行的分支引用使用 IDEA 风格的轮廓标签：HEAD 黄色、远端紫色、本地绿色，当前检出的本地分支也保持绿色。HEAD 与本地分支同指一个提交时显示绿＋黄两个图标；上游也同指该提交时显示紫＋绿＋黄，名称合为 `origin & main`。图标间隔一格，窄列缩短名称时保留整组图标。

右下提交详情始终逐行显示全部包含分支，使用相同的标签图标和颜色；只有 HEAD 为黄色，本地分支均为绿色，远端分支为紫色。分支较多时用滚轮或详情区的 `j/k`、翻页键浏览，无需 Show all／Show less 或空格键展开。

**Log 路径筛选：** 点 **Paths** 或按 `p`，选择一种方式：

- **Select...**：每行输入或粘贴一个文件或目录的相对路径，按 `Enter` 换行，`Ctrl-S`、`Ctrl-Enter`、`F2` 或点 **Apply** 应用。比如两行 `src` 和 `README.md` 会显示勾选仓库中改过任一路径的提交。
- **Select in Tree...**：第一层是全局 Roots 范围内识别到的仓库。点箭头展开目录，点复选框勾选整仓、目录或文件；键盘用上下键移动、左右键收展、空格勾选。按 `Enter` 或点 **OK** 生效，`Esc` 放弃草稿。同名文件只匹配树中勾选的仓库。
- **Roots**：点仓库行或按空格切换本页范围，立即生效；点标题或按 `Ctrl-A` 切换当前列出的全部仓库。也可在菜单直接输入完整仓库名并回车，只看该仓。点 Paths 旁的清除按钮会重置这一页的仓库与路径。

**Log 日期筛选：** 点击 Date 或按 `d`，菜单依次为 `Select...`、`Last 24 hours`、`Last 7 days`。首项打开自定义范围，在 From / To 中输入 `YYYY-MM-DD`，按 `Tab` 切换、`Ctrl-U` 清空当前字段、`Enter` 应用；范围包含本地日期的首尾两天。菜单可用方向键或 `j/k` 选择，`Esc` 取消，Date 旁的清除按钮只移除日期条件。

**Log 列表导航与复制：** `j/k` 或上下键在可视区域内移动，到底部／顶部后继续移动才滚动；列表不再自动居中。`gg/G` 到第一／最后一条，`H/M/L` 到当前屏幕的顶部／中间／底部提交。`Ctrl-F/B` 向下／上翻整页，`Ctrl-D/U` 翻半页，`PageDown/PageUp` 同整页；页大小按实际可见行数计算。`yy` 复制完整 commit ID，`ym` 复制完整提交信息（含正文），`ya` 复制 `姓名 <邮箱>`。这些键位用于提交列表；输入框和菜单保留各自含义。复制使用系统剪贴板：macOS 为 `pbcopy`，Windows 为 `clip.exe`，Linux 按顺序尝试 `wl-copy`、`xclip`、`xsel`；缺少可用命令或显示会话时会提示失败。

| 面板 | 实际操作 |
| --- | --- |
| **Log** | [三栏布局参考](docs/reference/IGit-Log-Graph.png)。同名分支跨仓合并；提交行默认从左至右是作者、本地日期与时间、彩色分支图、标题、引用。按住 `j/k` 可连续浏览，停在一行后右栏加载该提交的文件和包含分支；Diff 文件树以文件名颜色区分变更类型，不再追加状态字母。按 Enter 或双击文件打开 Diff。用 `/` 搜文本或哈希；点击 Branch/User/Date/Paths 或按 `b/a/d/p` 打开筛选菜单。菜单打开时，点另一个筛选就直接切换，点提交行就直接选中。`x` 清筛选，`+` 加载更多。 |
| **Commit** | [平铺布局参考](docs/reference/IGit-Commit-Flat.png)。默认按分组和仓库平铺，一行一个文件，灰色父路径区分同名文件。点行浏览、点框或按 `Space` 勾选、点箭头收展。双击文件或按 `Enter` 看补丁；`v` 开关预览，`o` 打开显示选项，`t` 切目录树。`i` 写当前仓的说明，`[/]` 换仓，`F2/Ctrl-S` 保存；`c` 审查提交，`P` 审查提交后进入 Push 预览。 |
| **Push** | 左侧仓名、分支、目标和 submodule 按列对齐；右侧默认全高文件树，目录带文件数，新增绿／修改蓝／删除红／重命名琥珀。当前仓库没有待推提交或没有选中行时，右侧居中显示 `No commits selected`。点眼睛 **Group By** 或按 `o`，可分别勾选 Directory、Repository、Module（默认全开）；关闭 Directory 后平铺文件并显示父路径。点箭头或 `h/l` 收展，文件区 `[/]` 全部收展；点文件或 `Enter` 看 Diff，`d` 开关详情、`Esc` 关闭。点框或 `Space` 勾仓；`t` 改 `remote:branch`（`Ctrl-U` 清输入），`v/x` 切 Follow tags/hooks；`p` 审查并推送，`R` 仅重试可见的失败仓，`f` Fetch。网络操作在后台运行，失败原因按仓保留。每仓最多展开最近 60 条，但计数和审查基于完整分支 tip。 |
| **Merge** | 先列当前选区内各仓库的冲突路径。Enter 或双击文件进入左 Current、中 Result、右 Incoming；`j/k` 换块、`h/l` 换栏、`[c` / `]c` 跳冲突。点顶部按钮或 `a` 只应用确定的非冲突变更；点沟槽箭头或 `H/L` 取左/右，`b` 先左后右，`i` 手改当前块，`Ctrl-S` 或 `Ctrl-Enter` 确认，`u` 撤销。用 `A` 审查并应用文件，`C` 单独继续仓库的合并操作。会话草稿不等同于已写入磁盘。 |

**对照 IDEA 的 Log：** 从源码目录运行下面两行，可在两个临时仓库上查看真实分叉与合并。按 `a` 输入 `me`、回车，再按 `p` 输入 `atlas-web`、回车。浅蓝底表示当前分支可达，亮蓝是选中提交；同仓的不同轨道各有颜色。按 `Ctrl-T` 新开一个无筛选 Log，`[/]` 切换并保留原页的筛选和提交位置。与 IDEA 的单仓日志逐行对照时，两边的 **Paths** 选同一个仓库名。IGit 右栏显示的是当前提交所属仓，并不限定整页历史；无需更改全局 Roots。图边缘的 `◀/▶` 表示该行的提交在屏外，选中它会平移整幅图。

```sh
log_workspace="$(sh examples/demo-log.sh)"
cargo run --locked --bin igit -- "$log_workspace"
```

提交点在自己的轨道中央。默认模式下，[kitty 0.36.2 及以上](https://github.com/kovidgoyal/kitty/blob/v0.36.2/kitty/fonts/box_drawing.py#L1334-L1365)和 [Ghostty](https://github.com/ghostty-org/ghostty/blob/v1.0.0/src/font/sprite/Box.zig#L1452) 可把圆点和线画在同一格。通过 tmux 使用兼容终端时，可用 `IGIT_GRAPH_GLYPHS=connected igit` 启用连线字形，`IGIT_GRAPH_GLYPHS=unicode igit` 只把图节点切回标准圆点。显式选择 `--icons unicode` 或 `--icons ascii` 优先于这项图设置：Unicode 用 `●` / `◉`，ASCII 用 `*` / `@` 表示普通提交 / HEAD。

**练习路径：** 在 Commit 给两个普通仓库各写一条说明，按 `P` 核对补丁并确认，再在 Push 按 `p` 检查两个目标后推送。在 Merge 打开子模块的 `transport.conf`，先按 `a`；路径和重试次数可以自动合入，transport 的选择仍需手动完成。应用文件后 Git 的 MERGE_HEAD 仍存在，直到单独确认 Continue。

**对照 IDEA 的 Commit：** 从源码目录运行下面两行，按 `2`。演练仓有 11 个默认勾选的跟踪变更和 8 个展开但未勾选的未跟踪文件。对照两个 `index.vue` 的灰色目录，分别打开 Diff；再查看 `iframe-proxy.mjs`，它只预览待提交的暂存版本。

```sh
commit_demo="$(sh examples/demo-commit.sh)"
cargo run --locked --bin ig -- "$commit_demo"
```

从另一个终端测试本次本地构建：在源码目录先运行 `cargo build --locked --bins`，然后在待测试目录运行 **`/源码绝对路径/target/debug/ig .`**，按 `2`。没有安装或覆盖全局命令也可以这样测试。

复选框分别表示全选、半选、未选、不可选（Unicode：`☑ / ⊟ / ☐ / ⊠`；ASCII：`[x] / [-] / [ ] / [!]`）。仓库和目录使用展开／收起的文件夹图标，子模块保留专用标记。点工具栏眼睛按钮或按 `o` 打开 **View Options**：Group By 中的 **Directory** 与 **Module**（仓库）独立切换，默认仅开启 Module；Show 中可开启 **Ignored Files** 和 **Git Status**。`j/k`、方向键、滚轮或悬停选择，`Space/Enter` 即时切换，`Esc` 关闭。忽略项默认隐藏，显示后为只读；忽略目录合并为一项，不读取其内容，不参与勾选、提交、移组或 AI 补丁。显示选项仅保留在本次会话，切换不改变原有勾选。

`s` 切换 Git 的 XY 状态（如 `AM`）；设置非空 `NO_COLOR` 时文件行直接显示 XY。`z/Z` 全部收起／展开，`u` 撤销上次勾选。`n/m` 建组／移组；也可以右键移组、将跟踪文件拖到另一分组。`a` 切换消息区当前仓的 Amend。工具栏 Diff 按钮或 `v` 打开预览；92 列起并排，更窄时全宽打开，按 `Esc` 回到原行。

**对照 IDEA 的 Push：** 运行下面两行，按 `3`，对照[文件树](docs/reference/IGit-Push-Tree.png)和[分组菜单实图](docs/reference/IGit-Push-Group-By.png)。演练仓包含 moss-web、10 个真实子模块和本机 bare 远端；moss-web 有 2 条待推提交、20 个改动文件，mes-isg 有 3 条待推提交。按 `o` 用 `Space` 关闭 Directory、`Esc` 收起菜单，分别点开父目录为 en_US 和 zh_CN 的 `system.ts` 检查 Diff；按 `p` 审查后用 `Esc` 取消。

```sh
push_demo="$(sh examples/demo-push.sh)"
cargo run --locked --bin ig -- "$push_demo"
```

## 提交与推送的实际语义

- 每仓一次 Git commit 或 push，批次无跨仓原子性。项目不会自动撤销已经成功的仓库。
- 按文件选提交，已暂存文件取 **index 版本**；未暂存文件取 **工作区版本**。例如同一文件先暂存了 A 行，又修改了 B 行，本次只提交 A。真实 index 中未勾选的暂存项保留。已跟踪文件换成同名目录时，只选旧文件的删除项不会带上新目录里的文件，AI 补丁也遵守这个范围。首版没有逐行手动勾选或把同一个文件拆到多个 changelist。
- 审查页显示本次提交的目标分支。从另一终端切换分支后可按 `F5` 更新列表；若在审查后切换分支，即使两个分支指向同一提交，也须重新审查。HEAD、勾选补丁或暂存版本变化同样需要重审。
- 提交执行真实 hooks。Amend 不允许更改已知远端跟踪引用包含的提交；本机跟踪记录不是对服务器发布状态的绝对证明。签名、hooks 或凭据助手若需要交互，使用已在系统配置好的 agent / pinentry；工作台不会收集 Git 密码。
- 推送选定的本地 SHA 到明确的远端分支。先核对远端 tip 和快进关系，再要求服务器仅在该 tip 仍匹配时更新；不提供强制覆盖历史的按钮。目标或远端发生变化时先 Fetch，必要时在外部整合上游后重新审查。Follow tags 只跟随 Git 定义的可达 annotated tags，不是推送所有标签。
- 父库和子库一起推送时先处理子库；子库失败会阻止依赖它的父库继续。修改子模块指针或只更换 `.gitmodules` 中的来源，都要验证子提交能从该版本声明的来源获取。中间提交也要通过；无法验证会阻止父库推送，不会擅自发布没勾选的子库。
- Merge 面板解决**已存在**的 Git 冲突，并支持继续相应操作；本版不是分支管理器，不负责发起任意 merge/rebase。普通 UTF-8 文本按 Git index 的三个 stage 构建，rebase 会注明新基底和重放提交。文件大小限 2 MiB；二进制、删除/修改、特殊文件模式、非标准过滤器及有外部手工改动的冲突文件均保留原状并提示外部处理。

## AI 提交说明（可选）

在 Commit 按 `S` 配置完整的 OpenAI-compatible chat completions URL、模型和提示词。可使用 HTTPS 服务，或带端口的本机 HTTP 服务。密钥从环境变量 `IGIT_LLM_API_KEY` 读取，不写入配置文件。需要系统可执行文件 `curl`。

按 `A` 审查要发送的仓库/文件及补丁大小；确认后才向所配服务逐仓请求，使用本次选中的实际补丁。候选不会自动替换原来的消息，也不会自动提交。没有模型服务时仍可完成全部 Git 操作。

本地分组、逐仓提交草稿与 AI 设置保存在 `IGIT_STATE_DIR` 或 `XDG_STATE_HOME/igit`（默认 `~/.local/state/igit`）下的工作区子目录，默认不往项目写入配置。

## 验证

```sh
cargo test --locked
cargo fmt --check
```

测试使用新建的临时仓库、本地 bare 远端和真实 git；覆盖分支图、部分暂存、过期审查、hook 拒绝、跨仓推送与重试、子模块、三方合并及不会覆写外部编辑等路径。执行二进制需要 Git；构建需要 Rust/Cargo。当前发行成品为 macOS Apple Silicon，其他平台需自行构建并验证终端输入。
