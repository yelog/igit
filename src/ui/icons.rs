//! Glyphs are a presentation preference. Callers measure the returned text in
//! terminal cells; no entry includes padding or changes a Git path or status.
use super::idea::{BLUE, DIM, GREEN, INK};
use nerd_font_symbols::{cod, md, oct, seti};
use ratatui::style::Color;
use std::{path::Path, str::FromStr};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum IconMode {
    #[default]
    NerdFont,
    Unicode,
    Ascii,
}

impl FromStr for IconMode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "nerd-font" => Ok(Self::NerdFont),
            "unicode" => Ok(Self::Unicode),
            "ascii" => Ok(Self::Ascii),
            _ => Err("expected nerd-font, unicode or ascii"),
        }
    }
}

// Keep all three representations together so a new role cannot omit a fallback.
macro_rules! glyphs {
    ($($role:ident => $nerd:expr, $unicode:literal, $ascii:literal;)+) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum Icon { $($role,)+ }

        impl IconMode {
            pub(crate) const fn glyph(self, icon: Icon) -> &'static str {
                let forms = match icon { $(Icon::$role => ($nerd, $unicode, $ascii),)+ };
                match self {
                    Self::NerdFont => forms.0,
                    Self::Unicode => forms.1,
                    Self::Ascii => forms.2,
                }
            }
        }
        #[cfg(test)]
        const ALL: &[Icon] = &[$(Icon::$role,)+];
    };
}

glyphs! {
    Checked     => md::MD_CHECKBOX_MARKED, "☑", "[x]";
    Unchecked   => md::MD_CHECKBOX_BLANK_OUTLINE, "☐", "[ ]";
    Partial     => md::MD_CHECKBOX_INTERMEDIATE, "⊟", "[-]";
    Locked      => cod::COD_LOCK, "⊠", "[!]";
    Expanded    => cod::COD_CHEVRON_DOWN, "▾", "v";
    Collapsed   => cod::COD_CHEVRON_RIGHT, "▸", ">";
    Repo        => cod::COD_REPO, "■", "R";
    Submodule   => cod::COD_FILE_SUBMODULE, "▣", "SM";
    Folder      => cod::COD_FOLDER, "▰", "D";
    FolderOpen  => cod::COD_FOLDER_OPENED, "▱", "D/";
    Changes     => cod::COD_FILES, "▤", "CL";
    Untracked   => cod::COD_NEW_FILE, "⊕", "?";
    Log         => cod::COD_HISTORY, "◷", "L";
    Commit      => cod::COD_GIT_COMMIT, "●", "*";
    Push        => cod::COD_REPO_PUSH, "⇡", "^";
    Merge       => cod::COD_GIT_MERGE, "⋈", "M";
    Branch      => oct::OCT_GIT_BRANCH, "├", "B";
    Remote      => cod::COD_REMOTE, "⇄", "R";
    Tag         => oct::OCT_TAG, "◇", "#";
    Head        => cod::COD_TARGET, "◉", "@";
    Current     => cod::COD_LOCATION, "●", "*";
    Favorite    => cod::COD_STAR_FULL, "★", "*";
    NotFavorite => cod::COD_STAR_EMPTY, "☆", "o";
    Ahead       => oct::OCT_ARROW_UP_RIGHT, "↗", "^";
    Behind      => oct::OCT_ARROW_DOWN_LEFT, "↙", "v";
    Search      => cod::COD_SEARCH, "⌕", "/";
    Filter      => cod::COD_FILTER, "▽", "F";
    Refresh     => cod::COD_REFRESH, "↻", "R";
    Undo        => md::MD_UNDO, "↶", "U";
    Move        => cod::COD_ARROW_SWAP, "⇄", "<>";
    Add         => cod::COD_ADD, "+", "+";
    Preview     => cod::COD_EYE, "◉", "V";
    ExpandAll   => cod::COD_EXPAND_ALL, "⊞", "[+]";
    CollapseAll => cod::COD_COLLAPSE_ALL, "⊟", "[-]";
    Previous    => cod::COD_CHEVRON_LEFT, "‹", "<";
    Next        => cod::COD_CHEVRON_RIGHT, "›", ">";
    Back        => cod::COD_ARROW_LEFT, "←", "<";
    Close       => cod::COD_CLOSE, "×", "x";
    Settings    => cod::COD_SETTINGS_GEAR, "⚙", "cfg";
    Ai          => cod::COD_SPARKLE, "✧", "AI";
    More        => cod::COD_ELLIPSIS, "⋯", "...";
    Help        => cod::COD_QUESTION, "?", "?";
    Enter       => md::MD_KEYBOARD_RETURN, "↵", "Enter";
    Up          => cod::COD_ARROW_UP, "↑", "^";
    Down        => cod::COD_ARROW_DOWN, "↓", "v";
    Left        => cod::COD_ARROW_LEFT, "←", "<-";
    Right       => cod::COD_ARROW_RIGHT, "→", "->";
    Success     => cod::COD_PASS, "✓", "ok";
    Error       => cod::COD_ERROR, "✗", "x";
    Warning     => cod::COD_WARNING, "!", "!";
    Conflict    => cod::COD_WARNING, "!", "!";
    Cursor      => "▏", "▏", "|";
    Dot         => "·", "·", ".";
    Divider     => "│", "│", "|";
    TreeEnd     => "└", "└", "`-";
    Diff        => cod::COD_DIFF, "⇆", "+-";
    Console     => cod::COD_TERMINAL, ">_", ">_";

    GenericFile => cod::COD_FILE, "▤", "F";
    TypeScript  => seti::SETI_TYPESCRIPT, "TS", "TS";
    JavaScript  => seti::SETI_JAVASCRIPT, "JS", "JS";
    Vue         => seti::SETI_VUE, "V", "V";
    Json        => seti::SETI_JSON, "{}", "{}";
    Markdown    => seti::SETI_MARKDOWN, "M↓", "MD";
    Config      => seti::SETI_CONFIG, "≡", "CF";
    Rust        => seti::SETI_RUST, "Rs", "Rs";
    Docker      => seti::SETI_DOCKER, "▦", "DK";
    GitFile     => seti::SETI_GIT, "G", "G";
    Python      => seti::SETI_PYTHON, "Py", "Py";
    Go          => seti::SETI_GO, "Go", "Go";
    ShellScript => seti::SETI_SHELL, "$", "$";
    Html        => seti::SETI_HTML, "<>", "<>";
    Css         => seti::SETI_CSS, "#", "#";
    Sql         => seti::SETI_DB, "DB", "DB";
    Lockfile    => seti::SETI_LOCK, "§", "LK";
}

impl IconMode {
    pub(crate) const fn check(self, checked: bool) -> &'static str {
        self.glyph(if checked {
            Icon::Checked
        } else {
            Icon::Unchecked
        })
    }

    pub(crate) const fn disclosure(self, open: bool) -> &'static str {
        self.glyph(if open {
            Icon::Expanded
        } else {
            Icon::Collapsed
        })
    }

    pub(crate) const fn folder(self, open: bool) -> &'static str {
        self.glyph(if open { Icon::FolderOpen } else { Icon::Folder })
    }

    pub(crate) const fn repo(self, submodule: bool) -> &'static str {
        self.glyph(if submodule {
            Icon::Submodule
        } else {
            Icon::Repo
        })
    }

    /// Use only the basename or extension, including when a parent or stem is
    /// not valid UTF-8. These colours describe file types, not Git statuses.
    pub(crate) fn file(self, path: &Path) -> (&'static str, Color) {
        use Icon::*;
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let (kind, tint) = match name {
            "Dockerfile"
            | ".dockerignore"
            | "compose.yml"
            | "compose.yaml"
            | "docker-compose.yml"
            | "docker-compose.yaml" => (Docker, BLUE),
            _ if name.starts_with("Dockerfile.") => (Docker, BLUE),
            ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => (GitFile, RUST),
            ".editorconfig" | ".env" => (Config, DIM),
            _ if name.starts_with(".env.") => (Config, DIM),
            _ => {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                match ext.to_ascii_lowercase().as_str() {
                    "ts" | "tsx" | "mts" | "cts" => (TypeScript, BLUE),
                    "js" | "jsx" | "cjs" | "mjs" => (JavaScript, GOLD),
                    "vue" => (Vue, GREEN),
                    "json" | "jsonc" => (Json, PURPLE),
                    "md" | "mdx" | "markdown" => (Markdown, BLUE),
                    "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" => (Config, DIM),
                    "rs" => (Rust, RUST),
                    "py" | "pyi" => (Python, GOLD),
                    "go" => (Go, BLUE),
                    "sh" | "bash" | "zsh" | "fish" => (ShellScript, GREEN),
                    "html" | "htm" => (Html, RUST),
                    "css" | "scss" | "sass" | "less" => (Css, PURPLE),
                    "sql" => (Sql, BLUE),
                    "lock" => (Lockfile, DIM),
                    _ => (GenericFile, INK),
                }
            }
        };
        (self.glyph(kind), tint)
    }
}

const GOLD: Color = Color::Rgb(225, 190, 98);
const PURPLE: Color = Color::Rgb(185, 146, 240);
const RUST: Color = Color::Rgb(215, 169, 127);

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn fallbacks_are_printable_without_a_patched_font() {
        for &icon in ALL {
            for mode in [IconMode::NerdFont, IconMode::Unicode, IconMode::Ascii] {
                let glyph = mode.glyph(icon);
                assert!(
                    glyph.width() > 0 && !glyph.chars().any(char::is_control),
                    "{icon:?}"
                );
                if mode == IconMode::Ascii {
                    assert!(glyph.is_ascii(), "{icon:?}: {glyph}");
                } else if mode == IconMode::Unicode {
                    assert!(
                        !glyph.chars().any(|c| matches!(c as u32,
                        0xe000..=0xf8ff | 0xf0000..=0xffffd | 0x100000..=0x10fffd)),
                        "{icon:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn file_identity_uses_the_basename_before_the_extension() {
        let icons = IconMode::NerdFont;
        for (path, want) in [
            ("infra/compose.yaml", seti::SETI_DOCKER),
            ("infra/values.yaml", seti::SETI_CONFIG),
            ("nested/.gitignore", seti::SETI_GIT),
            ("nested/.env.development.example", seti::SETI_CONFIG),
            ("src/界面/App.TSX", seti::SETI_TYPESCRIPT),
            ("folder.rs/README", cod::COD_FILE),
        ] {
            assert_eq!(icons.file(Path::new(path)).0, want);
        }
        #[cfg(unix)]
        {
            use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
            assert_eq!(
                icons.file(Path::new(OsStr::from_bytes(b"bad\xff.rs"))).0,
                seti::SETI_RUST
            );
        }
    }
}
