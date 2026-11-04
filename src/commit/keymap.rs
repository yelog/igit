use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl CommitPanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.view_menu.is_some() {
            return HelpContent::new(
                Section::new(
                    "Commit / 显示选项",
                    &[
                        ("j", "选择下一个显示选项"),
                        ("k", "选择上一个显示选项"),
                        ("Tab", "选择下一个显示选项"),
                        ("Shift-Tab", "选择上一个显示选项"),
                        ("g", "选择首个显示选项"),
                        ("G", "选择末个显示选项"),
                        ("Enter", "切换当前显示选项"),
                        ("Space", "切换当前显示选项，保留菜单与文件勾选"),
                        ("Esc", "关闭显示选项"),
                        ("o", "关闭显示选项"),
                    ],
                ),
                vec![],
            );
        }
        if let Some(edit) = &self.edit {
            let section = match edit {
                Edit::Move => Section::new(
                    "Commit / 移动到分组",
                    &[
                        ("j", "选择下一个变更分组"),
                        ("k", "选择上一个变更分组"),
                        ("Enter", "将当前范围移动到选定分组"),
                        ("Esc", "取消移动"),
                    ],
                ),
                Edit::AIDraft => Section::new(
                    "Commit / AI 候选审查",
                    &[
                        ("j", "向下滚动候选消息"),
                        ("k", "向上滚动候选消息"),
                        ("Enter", "采用候选稿，替换对应仓库的说明"),
                        ("Esc", "放弃候选稿，保留原说明"),
                    ],
                ),
                Edit::Group => Section::new(
                    "Commit / 新建分组",
                    &[
                        ("Left", "光标左移"),
                        ("Right", "光标右移"),
                        ("Home", "光标到行首"),
                        ("End", "光标到行尾"),
                        ("Backspace", "删除光标前的字符"),
                        ("Delete", "删除光标后的字符"),
                        ("Enter", "保存新分组"),
                        ("Esc", "取消新建分组"),
                    ],
                ),
                Edit::Message | Edit::AISettings => Section::new(
                    if matches!(edit, Edit::Message) {
                        "Commit / 编辑提交说明"
                    } else {
                        "Commit / AI 设置"
                    },
                    &[
                        ("Left", "光标左移"),
                        ("Right", "光标右移"),
                        ("Up", "光标上移"),
                        ("Down", "光标下移"),
                        ("Home", "光标到行首"),
                        ("End", "光标到行尾"),
                        ("Enter", "插入换行"),
                        ("Backspace", "删除光标前的字符"),
                        ("Delete", "删除光标后的字符"),
                        ("Ctrl-s", "保存当前输入"),
                        ("F2", "保存当前输入"),
                        ("Esc", "清除选区；无选区时取消输入"),
                    ],
                ),
            };
            let extra = if matches!(edit, Edit::Message) {
                vec![Section::new(
                    "Commit / 文本选择",
                    &[
                        ("Ctrl-a", "全选当前说明"),
                        ("Shift-Left", "向左扩展选区"),
                        ("Shift-Right", "向右扩展选区"),
                        ("Shift-Up", "向上扩展选区"),
                        ("Shift-Down", "向下扩展选区"),
                        ("Shift-Home", "选择到行首"),
                        ("Shift-End", "选择到行尾"),
                    ],
                )]
            } else {
                vec![]
            };
            return HelpContent::new(section, extra)
                .with_selection(self.selected_text().is_some(), false);
        }
        let current = match self.focus {
            Focus::Tree => Section::new(
                "Commit / 变更清单",
                &[
                    ("j", "选择下一个变更，高亮只改变预览对象"),
                    ("k", "选择上一个变更"),
                    ("Space", "勾选或取消当前文件、仓库或分组"),
                    ("Enter", "文件打开 Diff；容器收起或展开"),
                    ("h", "收起容器或到父节点"),
                    ("l", "展开容器或到子节点"),
                    ("g", "选择清单首项"),
                    ("G", "选择清单末项"),
                    ("PgDn", "向下翻页"),
                    ("PgUp", "向上翻页"),
                    ("m", "将当前范围移动到其他变更分组"),
                ],
            ),
            Focus::Diff => Section::new(
                "Commit / 预览",
                &[
                    ("Ctrl-a", "选择预览的全部文本"),
                    ("Esc", "清除选区；无选区时返回清单"),
                    ("j", "向下滚动 Diff 或范围摘要"),
                    ("k", "向上滚动 Diff 或范围摘要"),
                    ("g", "到预览顶部"),
                    ("G", "到预览底部"),
                    ("PgDn", "向下翻页"),
                    ("PgUp", "向上翻页"),
                    ("Enter", "返回变更清单的原位置"),
                    ("v", "关闭预览"),
                ],
            ),
            Focus::Message => Section::new(
                "Commit / 提交说明",
                &[
                    ("Enter", "编辑当前仓库的提交说明"),
                    ("[", "切换到上一个仓库的说明"),
                    ("]", "切换到下一个仓库的说明"),
                    ("a", "切换当前仓库的 Amend"),
                    ("A", "审查补丁后生成 AI 候选消息"),
                    ("S", "编辑 AI 端点、模型与提示词"),
                ],
            ),
        };
        HelpContent::new(
            current,
            vec![Section::new(
                "Commit / 面板通用",
                &[
                    ("Tab", "切换到下一个区域"),
                    ("Shift-Tab", "切换到上一个区域"),
                    ("v", "显示或隐藏当前行的预览"),
                    ("o", "打开 View Options 显示选项：目录、仓库分组与忽略文件"),
                    ("t", "切换平铺清单与目录分组"),
                    ("s", "显示或隐藏 Git 双列状态"),
                    ("z", "收起全部容器"),
                    ("Z", "展开全部容器"),
                    ("u", "撤销上一次复选框改动"),
                    ("n", "新建变更分组"),
                    ("i", "编辑当前仓库的提交说明"),
                    ("[", "切换到上一个仓库的说明"),
                    ("]", "切换到下一个仓库的说明"),
                    ("a", "切换当前仓库的 Amend"),
                    ("A", "审查并生成 AI 候选消息"),
                    ("S", "打开 AI 设置"),
                    ("c", "审查可见范围内已勾选文件的提交"),
                    ("P", "审查提交，完成后进入 Push 预览"),
                    ("Esc", "返回变更清单"),
                ],
            )],
        )
        .with_selection(self.selected_text().is_some(), true)
    }
}
