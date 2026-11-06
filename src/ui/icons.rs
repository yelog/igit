//! Glyphs are a presentation preference. Callers measure the returned text in
//! terminal cells; no entry includes padding or changes a Git path or status.
use super::idea::{BLUE, DIM, GREEN, INK, RED};
use nerd_font_symbols::{cod, dev, md, oct, seti};
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
    AutoResolve => cod::COD_WAND, "⚡", "*";
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
    Divider     => "│", "│", "|";
    Diff        => cod::COD_DIFF, "⇆", "+-";

    GenericFile => cod::COD_FILE, "▤", "F";
    TypeScript  => seti::SETI_TYPESCRIPT, "TS", "TS";
    JavaScript  => seti::SETI_JAVASCRIPT, "JS", "JS";
    ReactTs     => seti::SETI_REACT, "TSX", "TSX";
    ReactJs     => seti::SETI_REACT, "JSX", "JSX";
    Vue         => seti::SETI_VUE, "V", "V";
    Svelte      => seti::SETI_SVELTE, "Sv", "Sv";
    Astro       => seti::CUSTOM_ASTRO, "As", "As";
    Java        => seti::SETI_JAVA, "Jv", "Jv";
    Kotlin      => seti::CUSTOM_KOTLIN, "Kt", "Kt";
    Scala       => seti::SETI_SCALA, "Sc", "Sc";
    Groovy      => dev::DEV_GROOVY, "Gy", "Gy";
    C           => seti::SETI_C, "C", "C";
    Cpp         => seti::SETI_CPP, "C++", "C++";
    CSharp      => seti::SETI_C_SHARP, "C#", "C#";
    FSharp      => seti::SETI_F_SHARP, "F#", "F#";
    Swift       => seti::SETI_SWIFT, "Sw", "Sw";
    Dart        => seti::SETI_DART, "Da", "Da";
    Php         => seti::SETI_PHP, "PHP", "PHP";
    Ruby        => seti::CUSTOM_RUBY, "Rb", "Rb";
    Lua         => seti::SETI_LUA, "Lua", "Lua";
    Perl        => seti::SETI_PERL, "Pl", "Pl";
    R           => seti::SETI_R, "R", "R";
    Julia       => seti::SETI_JULIA, "Jl", "Jl";
    Elixir      => seti::CUSTOM_ELIXIR, "Ex", "Ex";
    Erlang      => dev::DEV_ERLANG, "Erl", "Erl";
    Haskell     => seti::SETI_HASKELL, "Hs", "Hs";
    Clojure     => seti::SETI_CLOJURE, "Clj", "Clj";
    Zig         => seti::SETI_ZIG, "Zig", "Zig";
    Json        => seti::SETI_JSON, "{}", "{}";
    Xml         => seti::SETI_XML, "XML", "XML";
    Markdown    => seti::SETI_MARKDOWN, "M↓", "MD";
    Config      => seti::SETI_CONFIG, "≡", "CF";
    Rust        => seti::SETI_RUST, "Rs", "Rs";
    Docker      => seti::SETI_DOCKER, "▦", "DK";
    GitFile     => seti::SETI_GIT, "G", "G";
    Python      => seti::SETI_PYTHON, "Py", "Py";
    Notebook    => seti::SETI_NOTEBOOK, "Nb", "Nb";
    Go          => seti::SETI_GO, "Go", "Go";
    ShellScript => seti::SETI_SHELL, "$", "$";
    PowerShell  => seti::SETI_POWERSHELL, "PS", "PS";
    Batch       => seti::CUSTOM_MSDOS, "Bat", "Bat";
    Html        => seti::SETI_HTML, "<>", "<>";
    Css         => seti::SETI_CSS, "#", "#";
    Sass        => seti::SETI_SASS, "Sass", "Sass";
    Sql         => seti::SETI_DB, "DB", "DB";
    Graphql     => seti::SETI_GRAPHQL, "GQL", "GQL";
    Terraform   => seti::SETI_TERRAFORM, "TF", "TF";
    Maven       => seti::SETI_MAVEN, "Mvn", "Mvn";
    Gradle      => seti::SETI_GRADLE, "Gr", "Gr";
    Makefile    => seti::SETI_MAKEFILE, "Mk", "Mk";
    CMake       => dev::DEV_CMAKE, "CM", "CM";
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
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Recognisable filenames and compound suffixes take precedence over
        // the final extension (e.g. pom.xml and build.gradle.kts).
        let (kind, tint) = match name.as_str() {
            "dockerfile"
            | "containerfile"
            | ".dockerignore"
            | "compose.yml"
            | "compose.yaml"
            | "docker-compose.yml"
            | "docker-compose.yaml" => (Docker, BLUE),
            _ if name.starts_with("dockerfile.") || name.starts_with("containerfile.") => {
                (Docker, BLUE)
            }
            ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => (GitFile, RUST),
            ".editorconfig" | ".env" => (Config, DIM),
            _ if name.starts_with(".env.") => (Config, DIM),
            ".bashrc" | ".bash_profile" | ".bash_login" | ".profile" | ".zshrc" | ".zshenv"
            | ".zprofile" | ".zlogin" | ".kshrc" => (ShellScript, GREEN),
            "pom.xml" => (Maven, RUST),
            _ if name.ends_with(".gradle.kts") => (Gradle, GREEN),
            "makefile" | "gnumakefile" => (Makefile, GREEN),
            "cmakelists.txt" => (CMake, BLUE),
            "gemfile" | "rakefile" | "vagrantfile" => (Ruby, RED),
            "jenkinsfile" => (Groovy, BLUE),
            "go.mod" | "go.sum" | "go.work" => (Go, BLUE),
            _ => {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                // .C and .H conventionally denote C++, unlike .c and .h.
                if matches!(ext, "C" | "H") {
                    return (self.glyph(Cpp), BLUE);
                }
                match ext.to_ascii_lowercase().as_str() {
                    "ts" | "mts" | "cts" => (TypeScript, BLUE),
                    "js" | "cjs" | "mjs" => (JavaScript, GOLD),
                    "tsx" => (ReactTs, BLUE),
                    "jsx" => (ReactJs, CYAN),
                    "vue" => (Vue, GREEN),
                    "svelte" => (Svelte, RUST),
                    "astro" => (Astro, RUST),
                    "java" => (Java, RUST),
                    "kt" | "kts" => (Kotlin, PURPLE),
                    "scala" | "sc" | "sbt" => (Scala, RED),
                    "groovy" | "gvy" | "gy" | "gsh" => (Groovy, BLUE),
                    "c" | "h" => (C, BLUE),
                    "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "h++" | "ipp" | "tpp"
                    | "inl" => (Cpp, BLUE),
                    "cs" | "csx" => (CSharp, GREEN),
                    "fs" | "fsi" | "fsx" => (FSharp, BLUE),
                    "swift" => (Swift, RUST),
                    "dart" => (Dart, BLUE),
                    "php" | "phtml" => (Php, PURPLE),
                    "rb" | "rake" | "gemspec" | "erb" => (Ruby, RED),
                    "lua" => (Lua, BLUE),
                    "pl" | "pm" | "t" => (Perl, BLUE),
                    "r" | "rmd" => (R, BLUE),
                    "jl" => (Julia, PURPLE),
                    "ex" | "exs" => (Elixir, PURPLE),
                    "erl" | "hrl" => (Erlang, RED),
                    "hs" | "lhs" => (Haskell, PURPLE),
                    "clj" | "cljs" | "cljc" | "edn" => (Clojure, GREEN),
                    "zig" | "zon" => (Zig, GOLD),
                    "json" | "jsonc" | "json5" => (Json, PURPLE),
                    "xml" | "xsd" | "xsl" | "xslt" | "dtd" | "xaml" | "plist" | "csproj"
                    | "fsproj" | "vbproj" => (Xml, RUST),
                    "md" | "mdx" | "markdown" => (Markdown, BLUE),
                    "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "properties" => {
                        (Config, DIM)
                    }
                    "rs" => (Rust, RUST),
                    "py" | "pyi" | "pyw" => (Python, GOLD),
                    "ipynb" => (Notebook, GOLD),
                    "go" => (Go, BLUE),
                    "sh" | "bash" | "zsh" | "fish" | "ksh" => (ShellScript, GREEN),
                    "ps1" | "psm1" | "psd1" => (PowerShell, BLUE),
                    "bat" | "cmd" => (Batch, GREEN),
                    "html" | "htm" => (Html, RUST),
                    "css" | "less" => (Css, PURPLE),
                    "scss" | "sass" => (Sass, PURPLE),
                    "sql" => (Sql, BLUE),
                    "graphql" | "gql" => (Graphql, PURPLE),
                    "tf" | "tfvars" => (Terraform, PURPLE),
                    "gradle" => (Gradle, GREEN),
                    "mk" | "mak" => (Makefile, GREEN),
                    "cmake" => (CMake, BLUE),
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
const CYAN: Color = Color::Rgb(97, 190, 208);

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
            ("src/ｕｉ/App.TSX", seti::SETI_REACT),
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
