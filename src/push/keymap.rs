use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl PushPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.group_menu.is_some() {
            return HelpContent::new(
                Section::new(
                    "Push / 文件分组",
                    &[
                        ("j", "选择下一个分组选项"),
                        ("k", "选择上一个分组选项"),
                        ("g", "选择第一项 Directory"),
                        ("G", "选择最后一项 Module"),
                        ("Space", "切换当前分组"),
                        ("Enter", "切换当前分组"),
                        ("Esc", "关闭 Group By 菜单"),
                    ],
                ),
                vec![],
            );
        }
        if self.target_edit.is_some() {
            return HelpContent::new(
                Section::new(
                    "Push / 编辑目标",
                    &[
                        ("Backspace", "删除目标文本的末尾字符"),
                        ("Ctrl-u", "清空目标文本"),
                        ("Enter", "校验并保存 remote:branch 推送目标"),
                        ("Esc", "取消编辑，保留原目标"),
                    ],
                ),
                vec![],
            );
        }
        let current = match self.focus {
            0 => Section::new(
                "Push / 仓库与提交",
                &[
                    ("j", "选择下一个仓库或提交"),
                    ("k", "选择上一个仓库或提交"),
                    ("Space", "勾选或取消当前仓库"),
                    ("h", "收起当前仓库的提交"),
                    ("l", "展开当前仓库的提交"),
                    ("Enter", "收展仓库；提交行进入变更文件区"),
                    ("t", "编辑当前仓库的 remote:branch 目标"),
                ],
            ),
            1 => Section::new(
                "Push / 变更文件",
                &[
                    ("j", "选择下一个变更文件"),
                    ("k", "选择上一个变更文件"),
                    ("h", "收起目录或到父节点"),
                    ("l", "展开目录或到子节点"),
                    ("Enter", "文件打开 Diff；目录收起或展开"),
                    ("[", "收起全部目录"),
                    ("]", "展开全部目录"),
                    ("d", "显示或隐藏当前文件的 Diff"),
                ],
            ),
            _ => Section::new(
                "Push / Diff 与详情",
                &[
                    ("Ctrl-a", "选择详情的全部文本"),
                    ("j", "向下滚动补丁或详情"),
                    ("k", "向上滚动补丁或详情"),
                    ("Esc", "清除选区；无选区时关闭详情"),
                    ("d", "关闭 Diff / 详情"),
                ],
            ),
        };
        HelpContent::new(
            current,
            vec![Section::new(
                "Push / 面板通用",
                &[
                    ("Tab", "切换到下一个区域"),
                    ("Shift-Tab", "切换到上一个区域"),
                    ("o", "打开 Group By 文件分组菜单"),
                    ("Space", "切换当前仓库的勾选"),
                    ("t", "编辑当前仓库的推送目标"),
                    ("p", "审查可见范围内已勾选仓库的推送计划"),
                    ("R", "只审查并重试失败的可见仓库"),
                    ("f", "获取已勾选仓库的远端引用"),
                    ("v", "切换跟随标签推送"),
                    ("x", "切换跳过推送 hooks"),
                    ("d", "显示或隐藏 Diff / 详情"),
                ],
            )],
        )
        .with_selection(self.selected_text().is_some(), true)
    }
}
