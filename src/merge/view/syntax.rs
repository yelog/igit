//! Small, dependency-free code accents. Unknown file types remain plain text.
//! Highlighting is display-only; the original revision bytes are kept for copy.
use ratatui::style::Color;
use std::ops::Range;

const KEYWORD: Color = Color::Rgb(207, 142, 109);
const STRING: Color = Color::Rgb(106, 171, 115);
const CALL: Color = Color::Rgb(86, 156, 214);
const NUMBER: Color = Color::Rgb(177, 148, 221);
const PROPERTY: Color = Color::Rgb(199, 125, 187);
const COMMENT: Color = Color::Rgb(104, 125, 101);

pub(super) fn tokens(path: &str, text: &str) -> Vec<(Range<usize>, Color)> {
    let extension = path.rsplit('.').next().unwrap_or("");
    if !matches!(
        extension,
        "js" | "jsx"
            | "ts"
            | "tsx"
            | "vue"
            | "rs"
            | "py"
            | "go"
            | "java"
            | "c"
            | "h"
            | "cpp"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "sh"
    ) || text.len() > 4096
    {
        return Vec::new();
    }
    let chars: Vec<_> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    let hash_comment = matches!(extension, "py" | "toml" | "yaml" | "yml" | "sh");
    while at < chars.len() {
        let start = at;
        let ch = chars[at];
        if (hash_comment && ch == '#')
            || (ch == '/' && matches!(chars.get(at + 1), Some('/' | '*')))
            || (ch == '*' && chars[..at].iter().all(|c| c.is_whitespace()))
        {
            out.push((at..chars.len(), COMMENT));
            break;
        }
        if matches!(ch, '\'' | '"' | '`') {
            at += 1;
            while at < chars.len() {
                let c = chars[at];
                at += 1;
                if c == '\\' {
                    at = (at + 1).min(chars.len());
                } else if c == ch {
                    break;
                }
            }
            out.push((start..at, STRING));
        } else if ch.is_ascii_digit() {
            at += 1;
            while at < chars.len()
                && (chars[at].is_ascii_alphanumeric() || matches!(chars[at], '.' | '_'))
            {
                at += 1;
            }
            out.push((start..at, NUMBER));
        } else if ch.is_alphabetic() || matches!(ch, '_' | '$') {
            at += 1;
            while at < chars.len()
                && (chars[at].is_alphanumeric() || matches!(chars[at], '_' | '$'))
            {
                at += 1;
            }
            let word: String = chars[start..at].iter().collect();
            let next = chars[at..].iter().find(|c| !c.is_whitespace());
            let color = if matches!(
                word.as_str(),
                "function"
                    | "const"
                    | "let"
                    | "var"
                    | "return"
                    | "if"
                    | "else"
                    | "new"
                    | "throw"
                    | "try"
                    | "catch"
                    | "finally"
                    | "for"
                    | "while"
                    | "do"
                    | "break"
                    | "continue"
                    | "switch"
                    | "case"
                    | "default"
                    | "class"
                    | "extends"
                    | "import"
                    | "export"
                    | "from"
                    | "async"
                    | "await"
                    | "true"
                    | "false"
                    | "null"
                    | "undefined"
                    | "typeof"
                    | "instanceof"
                    | "fn"
                    | "pub"
                    | "use"
                    | "mod"
                    | "impl"
                    | "struct"
                    | "enum"
                    | "match"
                    | "mut"
                    | "self"
                    | "def"
                    | "in"
                    | "and"
                    | "or"
                    | "not"
                    | "None"
                    | "True"
                    | "False"
                    | "func"
                    | "package"
                    | "type"
                    | "defer"
                    | "void"
                    | "int"
                    | "public"
                    | "private"
                    | "static"
            ) {
                Some(KEYWORD)
            } else if next == Some(&'(') {
                Some(CALL)
            } else if next == Some(&':') {
                Some(PROPERTY)
            } else {
                None
            };
            if let Some(color) = color {
                out.push((start..at, color));
            }
        } else {
            at += 1;
        }
    }
    out
}
