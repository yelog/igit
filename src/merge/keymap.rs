use super::*;
use crate::ui::help::{HelpContent, HelpSection as Section};

impl MergePanel {
    pub(crate) fn keymap(&self) -> HelpContent {
        if self.edit.is_some() {
            return HelpContent::new(
                Section::new(
                    "Merge / 结果编辑",
                    &[
                        ("Left", "光标左移"),
                        ("Right", "光标右移"),
                        ("Up", "光标上移"),
                        ("Down", "光标下移"),
                        ("Home", "光标到行首"),
                        ("End", "光标到行尾"),
                        ("Enter", "插入换行"),
                        ("Tab", "插入制表符"),
                        ("Backspace", "删除光标前的字符"),
                        ("Delete", "删除光标后的字符"),
                        ("Ctrl-s", "接受当前块的编辑结果"),
                        ("Ctrl-Enter", "接受当前块的编辑结果"),
                        ("Esc", "取消本次块编辑"),
                    ],
                ),
                vec![],
            );
        }
        if self.doc.is_none() {
            return HelpContent::new(
                Section::new(
                    "Merge / 冲突文件",
                    &[
                        ("j", "选择下一个冲突文件或仓库操作"),
                        ("k", "选择上一个冲突文件或仓库操作"),
                        ("g", "选择首项"),
                        ("G", "选择末项"),
                        ("Enter", "打开冲突文件或审查待继续的操作"),
                        ("C", "审查并继续当前仓库的 merge / rebase"),
                    ],
                ),
                vec![],
            );
        }
        let title = match self.side {
            0 => "Merge / 左侧来源",
            1 => "Merge / 合并结果",
            _ => "Merge / 右侧来源",
        };
        HelpContent::new(
            Section::new(
                title,
                &[
                    ("j", "选择下一个变更块"),
                    ("k", "选择上一个变更块"),
                    ("[c", "跳到上一个未解决冲突"),
                    ("]c", "跳到下一个未解决冲突"),
                    ("h", "切换到左侧区域"),
                    ("l", "切换到右侧区域"),
                    ("Tab", "切换到下一个区域"),
                    ("Shift-Tab", "切换到上一个区域"),
                    ("Left", "向左滚动三栏内容"),
                    ("Right", "向右滚动三栏内容"),
                    ("PgDn", "向下翻页，暂停跟随当前块"),
                    ("PgUp", "向上翻页，暂停跟随当前块"),
                    ("z", "切换当前栏全宽显示"),
                ],
            ),
            vec![Section::new(
                "Merge / 取舍与应用",
                &[
                    ("H", "当前块采用左侧内容"),
                    ("L", "当前块采用右侧内容"),
                    ("b", "当前冲突块保留双方内容"),
                    ("a", "应用全部非冲突块"),
                    ("i", "编辑当前块的合并结果"),
                    ("u", "撤销上一次取舍或编辑"),
                    ("s", "将文件草稿保存在本次会话中"),
                    ("A", "审查并应用当前文件，写入并暂存"),
                    ("Esc", "保存会话内草稿，返回冲突文件列表"),
                ],
            )],
        )
    }
}
