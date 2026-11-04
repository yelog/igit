//! Resolve branch expressions against each repository's actual refs before calling Git.
use super::{Branch, Result};
use crate::log::filters::short_ref;
use anyhow::{bail, ensure};
use regex_lite::Regex;
use std::collections::BTreeSet;

#[derive(Clone)]
pub(in crate::log) struct BranchFilter(Vec<Term>);

#[derive(Clone)]
struct Term {
    value: String,
    exclude: bool,
    pattern: Option<Regex>,
    range_start: Option<String>,
}

impl From<&str> for BranchFilter {
    fn from(name: &str) -> Self {
        Self::refs([name])
    }
}

impl BranchFilter {
    pub(in crate::log) fn refs<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self(
            names
                .into_iter()
                .map(|s| Term {
                    value: s.to_owned(),
                    exclude: false,
                    pattern: None,
                    range_start: None,
                })
                .collect(),
        )
    }

    pub(in crate::log) fn single(&self) -> Option<&str> {
        match self.0.as_slice() {
            [t] if !t.exclude && t.pattern.is_none() && t.range_start.is_none() => {
                Some(t.value.as_str())
            }
            _ => None,
        }
    }

    pub(in crate::log) fn text(&self, short: bool) -> String {
        self.0
            .iter()
            .map(|t| {
                let name = if short && t.pattern.is_none() {
                    short_ref(&t.value)
                } else {
                    &t.value
                };
                match &t.range_start {
                    Some(start) => {
                        format!("{}..{name}", if short { short_ref(start) } else { start })
                    }
                    None => format!("{}{name}", if t.exclude { "-" } else { "" }),
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    pub(in crate::log) fn parse(input: &str, known: &BTreeSet<&str>) -> Result<Option<Self>> {
        let mut terms = vec![];
        let resolve = |name: &str, exclude| -> Result<Term> {
            ensure!(!name.is_empty(), "缺少分支名");
            let mut matches = known.iter().copied().filter(|s| short_ref(s) == name);
            let (value, pattern) = if known.contains(name) {
                (name.to_owned(), None)
            } else if let Some(full) = matches.next() {
                ensure!(
                    matches.next().is_none(),
                    "多个引用同名，请选择完整引用：{name}"
                );
                (full.to_owned(), None)
            } else {
                let re = Regex::new(&format!("^(?:{name})$"))?;
                ensure!(
                    known.iter().any(|s| re.is_match(short_ref(s))),
                    "没有匹配的分支或标签：{name}"
                );
                (name.to_owned(), Some(re))
            };
            Ok(Term {
                value,
                exclude,
                pattern,
                range_start: None,
            })
        };
        let input = input.trim();
        if known.contains(input) {
            return Ok(Some(Self::from(input)));
        }
        for part in input
            .split(['|', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some((from, to)) = part.split_once("..") {
                if part.starts_with('-') || to.starts_with('.') {
                    bail!("请输入有效的起止分支：{part}");
                }
                let start = resolve(if from.is_empty() { "HEAD" } else { from }, false)?;
                let mut end = resolve(if to.is_empty() { "HEAD" } else { to }, false)?;
                ensure!(
                    start.pattern.is_none() && end.pattern.is_none(),
                    "范围两端须使用确切的引用名：{part}"
                );
                end.range_start = Some(start.value);
                terms.push(end);
            } else {
                terms.push(resolve(
                    part.strip_prefix('-').unwrap_or(part),
                    part.starts_with('-'),
                )?);
            }
        }
        Ok((!terms.is_empty()).then_some(Self(terms)))
    }

    // None means no positive ref exists in this root. Never fall back to all its commits.
    pub(super) fn tips(&self, refs: &[Branch]) -> Option<Vec<String>> {
        let mut include = BTreeSet::new();
        let mut exclude = BTreeSet::new();
        let known: BTreeSet<_> = std::iter::once("HEAD")
            .chain(refs.iter().map(|b| b.name.as_str()))
            .collect();
        for t in &self.0 {
            if let Some(start) = &t.range_start {
                if known.contains(start.as_str()) && known.contains(t.value.as_str()) {
                    include.insert(t.value.as_str());
                    exclude.insert(start.as_str());
                }
                continue;
            }
            let out = if t.exclude {
                &mut exclude
            } else {
                &mut include
            };
            if let Some(pat) = &t.pattern {
                out.extend(known.iter().copied().filter(|r| pat.is_match(short_ref(r))));
            } else if known.contains(t.value.as_str()) {
                out.insert(t.value.as_str());
            }
        }
        let mut args = if self.0.iter().any(|t| !t.exclude) {
            if include.is_empty() {
                return None;
            }
            vec!["--end-of-options".into()]
        } else {
            let mut all = super::all_tips();
            all.push("--end-of-options".into());
            all
        };
        args.extend(include.into_iter().map(str::to_owned));
        args.extend(exclude.into_iter().map(|s| format!("^{s}")));
        Some(args)
    }
}
