use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl LogPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        let modal = if self.diff.is_some() {
            Some(Section::new(
                "Log / Diff",
                &[
                    ("Ctrl-a", "选择当前 Diff 的全部文本"),
                    ("j", "向下滚动补丁"),
                    ("k", "向上滚动补丁"),
                    ("h", "向左滚动补丁"),
                    ("l", "向右滚动补丁"),
                    ("PgDn", "向下翻页"),
                    ("PgUp", "向上翻页"),
                    ("g", "到补丁首行"),
                    ("G", "到补丁末行"),
                    ("Esc", "清除选区；无选区时关闭 Diff"),
                ],
            ))
        } else if self.view_menu.is_some_and(|menu| menu.column.is_some()) {
            Some(Section::new(
                "Log / Columns",
                &[
                    ("j", "选择下一列"),
                    ("k", "选择上一列"),
                    ("K", "将当前列向左移一位"),
                    ("J", "将当前列向右移一位"),
                    ("Shift-Up", "将当前列向左移一位"),
                    ("Shift-Down", "将当前列向右移一位"),
                    ("Space", "显示或隐藏当前列"),
                    ("Enter", "显示或隐藏当前列"),
                    ("h", "返回显示选项"),
                    ("Esc", "返回显示选项"),
                    ("o", "关闭显示选项"),
                ],
            ))
        } else if self.view_menu.is_some() {
            Some(Section::new(
                "Log / 显示选项",
                &[
                    ("j", "选择下一项"),
                    ("k", "选择上一项"),
                    ("l", "展开 Columns 列选项"),
                    ("Enter", "切换当前选项"),
                    ("h", "返回上层或关闭菜单"),
                    ("o", "关闭显示选项"),
                    ("Esc", "返回上层或关闭菜单"),
                ],
            ))
        } else if let Some(menu) = &self.branch_menu {
            Some(if menu.text_entry() {
                Section::new(
                    "Log / Branch 搜索",
                    &[
                        ("Down", "选择下一条搜索候选"),
                        ("Up", "选择上一条搜索候选"),
                        ("Enter", "选择候选或应用输入的范围"),
                        ("Tab", "补全当前搜索候选"),
                        ("Ctrl-Enter", "应用手动输入的分支范围"),
                        ("Backspace", "删除末尾字符"),
                        ("Ctrl-u", "清空搜索文本"),
                        ("PgDn", "向下翻页"),
                        ("PgUp", "向上翻页"),
                        ("Esc", "返回分支菜单"),
                    ],
                )
            } else {
                Section::new(
                    "Log / Branch 筛选",
                    &[
                        ("j", "选择下一条分支"),
                        ("k", "选择上一条分支"),
                        ("h", "返回上层菜单"),
                        ("Right", "展开当前分支目录"),
                        ("Enter", "选择当前分支或目录"),
                        ("Space", "切换当前分支的收藏星标"),
                        ("/", "搜索分支或输入分支范围"),
                        ("Home", "选择首条分支"),
                        ("End", "选择末条分支"),
                        ("PgDn", "向下翻页"),
                        ("PgUp", "向上翻页"),
                        ("Esc", "返回上层或取消筛选"),
                    ],
                )
            })
        } else if self.editing_paths() {
            Some(match &self.path_dialog {
                Some(paths::Dialog::Text(_)) => Section::new(
                    "Log / Select Paths",
                    &[
                        ("Enter", "换行，每行输入一个相对路径"),
                        ("Up", "光标移到上一行"),
                        ("Down", "光标移到下一行"),
                        ("Home", "光标移到行首"),
                        ("End", "光标移到行尾"),
                        ("Ctrl-u", "清空路径草稿"),
                        ("F2", "应用输入的全部路径"),
                        ("Ctrl-s", "应用输入的全部路径"),
                        ("Ctrl-Enter", "应用输入的全部路径"),
                        ("Esc", "取消输入，保留原筛选"),
                    ],
                ),
                Some(paths::Dialog::Tree(_)) => Section::new(
                    "Log / Select in Tree",
                    &[
                        ("j", "移到下一行"),
                        ("k", "移到上一行"),
                        ("l", "展开目录或移到子项"),
                        ("h", "收起目录或移到父项"),
                        ("Space", "勾选或取消此仓库、目录或文件"),
                        ("PgDn", "向下翻页"),
                        ("PgUp", "向上翻页"),
                        ("Enter", "应用树中勾选的路径"),
                        ("Esc", "放弃此次勾选"),
                    ],
                ),
                None => Section::new(
                    "Log / Paths 筛选",
                    &[
                        ("/", "输入仓库名或路径快速查找"),
                        ("Down", "选择下一项"),
                        ("Up", "选择上一项"),
                        ("Enter", "打开选择器、应用输入或完成仓库勾选"),
                        ("Space", "打开选择器或切换当前仓库"),
                        ("Ctrl-a", "全选或取消当前列出的仓库"),
                        ("Ctrl-u", "清空菜单里的快速查找"),
                        ("Esc", "关闭 Paths 菜单"),
                    ],
                ),
            })
        } else if let Some((field, _)) = self.editor {
            let title = match field {
                Field::Refs => "Log / 分支搜索",
                Field::Text => "Log / 文本搜索",
                Field::Author => "Log / User 筛选",
                Field::Date => "Log / Date 筛选",
                Field::Path => "Log / Paths 筛选",
                Field::Branch => "Log / Branch 筛选",
            };
            let section = if field == Field::Date && self.date_range.is_none() {
                Section::new(
                    title,
                    &[
                        ("j", "选择下一个日期范围"),
                        ("k", "选择上一个日期范围"),
                        ("Enter", "应用快捷范围或打开日期输入"),
                        ("Esc", "关闭日期菜单"),
                    ],
                )
            } else if field == Field::Date {
                Section::new(
                    title,
                    &[
                        ("Tab", "切换起始日期与结束日期"),
                        ("Enter", "校验并应用日期范围"),
                        ("Backspace", "删除当前日期的末尾字符"),
                        ("Ctrl-u", "清空当前日期"),
                        ("Esc", "取消输入，保留原日期条件"),
                    ],
                )
            } else {
                Section::new(
                    title,
                    &[
                        ("Down", "选择下一条候选"),
                        ("Up", "选择上一条候选"),
                        ("Enter", "应用搜索或筛选"),
                        ("Backspace", "删除末尾字符"),
                        ("Ctrl-u", "清空当前输入"),
                        ("Esc", "取消输入，保留原条件"),
                    ],
                )
            };
            let mut content = HelpContent::new(section, vec![]);
            if field == Field::Text {
                content.sections.push(Section::new(
                    "文本匹配",
                    &[("Alt-r", "切换正则表达式匹配"), ("Alt-c", "切换大小写敏感")],
                ));
            }
            return content;
        } else {
            None
        };
        if let Some(section) = modal {
            return HelpContent::new(section, vec![])
                .with_selection(self.selected_text().is_some(), true);
        }
        let current = match self.focus {
            Pane::Graph => Section::new(
                "Log / 提交列表",
                &[
                    ("j", "选择下一条提交"),
                    ("k", "选择上一条提交"),
                    ("gg", "选择首条提交"),
                    ("G", "选择末条提交"),
                    ("H", "选择屏幕顶部的提交"),
                    ("M", "选择屏幕中间的提交"),
                    ("L", "选择屏幕底部的提交"),
                    ("Ctrl-f", "向下翻整页"),
                    ("Ctrl-b", "向上翻整页"),
                    ("Ctrl-d", "向下翻半页"),
                    ("Ctrl-u", "向上翻半页"),
                    ("yy", "复制当前提交的完整 ID"),
                    ("ym", "复制当前提交的完整消息"),
                    ("ya", "复制当前提交的作者姓名与邮箱"),
                    ("Enter", "打开当前提交的文件 Diff"),
                ],
            ),
            Pane::Refs => Section::new(
                "Log / 分支树",
                &[
                    ("j", "选择下一项分支或标签"),
                    ("k", "选择上一项分支或标签"),
                    ("h", "收起目录、到父节点或切换区域"),
                    ("l", "展开目录、到子节点或切换区域"),
                    ("Enter", "按当前分支或标签筛选历史"),
                    ("/", "搜索分支或标签"),
                    ("g", "选择首项"),
                    ("G", "选择末项"),
                    ("PgDn", "向下翻页"),
                    ("PgUp", "向上翻页"),
                ],
            ),
            Pane::Files => Section::new(
                "Log / 文件树",
                &[
                    ("j", "选择下一个变更文件"),
                    ("k", "选择上一个变更文件"),
                    ("h", "收起目录、到父节点或切换区域"),
                    ("l", "展开目录、到子节点或切换区域"),
                    ("Enter", "打开文件 Diff，或收展当前目录"),
                    ("z", "收起全部目录"),
                    ("Z", "展开全部目录"),
                    ("g", "选择首项"),
                    ("G", "选择末项"),
                    ("PgDn", "向下翻页"),
                    ("PgUp", "向上翻页"),
                ],
            ),
            Pane::Info => Section::new(
                "Log / 提交详情",
                &[
                    ("Ctrl-a", "选择当前提交详情的全部文本"),
                    ("Esc", "清除选区；无选区时返回提交列表"),
                    ("j", "向下滚动提交详情"),
                    ("k", "向上滚动提交详情"),
                    ("g", "到详情顶部"),
                    ("G", "到详情底部"),
                    ("PgDn", "向下翻页"),
                    ("PgUp", "向上翻页"),
                    ("h", "切换到上一个区域"),
                    ("l", "切换到下一个区域"),
                ],
            ),
        };
        HelpContent::new(
            current,
            vec![Section::new(
                "Log / 面板通用",
                &[
                    ("Tab", "切换到下一个区域"),
                    ("Shift-Tab", "切换到上一个区域"),
                    ("/", "搜索提交文本或哈希"),
                    ("b", "打开 Branch 筛选"),
                    ("a", "打开 User 筛选"),
                    ("d", "打开 Date 筛选"),
                    ("p", "打开 Paths 筛选"),
                    ("x", "清除当前 Log 页的全部筛选"),
                    ("o", "打开 Show / Highlight 显示选项"),
                    ("B", "显示或隐藏 Branch or Tag 分支栏"),
                    ("I", "显示或隐藏右侧检查栏"),
                    ("[", "切换到上一个 Log 页"),
                    ("]", "切换到下一个 Log 页"),
                    ("Ctrl-t", "新建 Log 页"),
                    ("Ctrl-w", "关闭当前 Log 页"),
                    ("+", "加载更多历史"),
                    ("Esc", "焦点返回提交列表"),
                ],
            )],
        )
        .with_selection(self.selected_text().is_some(), true)
    }
}
