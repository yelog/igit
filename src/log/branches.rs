//! Branch filter popup. The two menu levels share ref IDs; only choosing a name reads history.
mod view;

use super::{filters::short_ref, model::BranchFilter, *};
use anyhow::Context as _;
use crossterm::event::KeyEvent;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::ErrorKind,
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Main(usize),
    Child(usize),
    Match(usize),
}
#[derive(Clone, Copy)]
pub(super) enum Target {
    Row(Row),
    Star(Row),
    Seek(Row),
    Input,
    Apply,
    Back,
}
pub(super) enum Input<'a> {
    Key(&'a KeyEvent),
    Click(Target),
    Scroll(isize, Option<Row>),
    Paste(&'a str),
}
enum Outcome {
    Stay,
    Close,
    Apply(Option<BranchFilter>),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    Select,
    Favorites,
    Ref(usize),
    Divider,
    Group(usize),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Main,
    Child,
    Select,
}
struct Entry {
    name: String,
    default_star: bool,
}
struct Group {
    label: String,
    entries: Vec<usize>,
    width: u16,
}

// Compare digit runs without parsing an integer: release/2 comes before release/10.
fn ref_order(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn digits(s: &[u8]) -> (&[u8], &[u8]) {
        let (number, rest) = s.split_at(
            s.iter()
                .position(|c| !c.is_ascii_digit())
                .unwrap_or(s.len()),
        );
        (
            &number[number
                .iter()
                .position(|&c| c != b'0')
                .unwrap_or(number.len())..],
            rest,
        )
    }
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    while let (Some((&i, xx)), Some((&j, yy))) = (x.split_first(), y.split_first()) {
        let order = if i.is_ascii_digit() && j.is_ascii_digit() {
            let (n, rest_x) = digits(x);
            let (m, rest_y) = digits(y);
            x = rest_x;
            y = rest_y;
            n.len().cmp(&m.len()).then(n.cmp(m))
        } else {
            x = xx;
            y = yy;
            i.cmp(&j)
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}
pub(super) struct Menu {
    refs: Vec<Entry>,
    groups: Vec<Group>,
    main: Vec<Item>,
    starred: BTreeMap<String, bool>,
    file: PathBuf,
    row: usize,
    child: usize,
    main_top: usize,
    list_top: usize,
    level: Level,
    original: String,
    query: String,
    speed_search: bool,
    matches: Vec<usize>,
    picked: Option<usize>,
}

impl Menu {
    pub(super) fn text_entry(&self) -> bool {
        self.level == Level::Select
    }

    pub(super) fn open(hist: &History, filter: &Filter, c: &Context) -> Result<Self> {
        let file = c.state_dir.join("branch-favorites.json");
        let starred = match fs::read(&file) {
            Ok(data) => serde_json::from_slice(&data).context("无法读取分支收藏")?,
            Err(e) if e.kind() == ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e.into()),
        };
        let mut names = BTreeMap::from([("HEAD", true)]);
        // None is the Local group. Merge identical full refs across the visible repositories.
        let mut groups = BTreeMap::<Option<&str>, BTreeSet<&str>>::new();
        for b in &hist.branches {
            if !c.scope.contains(&b.root) || !filter.includes_root(b.root) {
                continue;
            }
            let name = b.name.as_str();
            let star = if let Some(local) = name.strip_prefix("refs/heads/") {
                groups.entry(None).or_default().insert(name);
                matches!(local, "main" | "master")
                    || hist.remote_upstream(b.root, local).is_some_and(|up| {
                        hist.remote_heads
                            .contains(&(b.root, format!("refs/remotes/{up}")))
                    })
            } else if let Some(remote_ref) = name.strip_prefix("refs/remotes/") {
                let Some((remote, branch)) = hist.split_remote(b.root, remote_ref) else {
                    continue;
                };
                groups.entry(Some(remote)).or_default().insert(name);
                matches!(branch, "main" | "master")
                    || hist.remote_heads.contains(&(b.root, b.name.clone()))
            } else {
                false
            }; // Tags remain available in Select… and in the reference tree.
            *names.entry(name).or_default() |= star;
        }
        let ids: BTreeMap<_, _> = names
            .keys()
            .copied()
            .enumerate()
            .map(|(i, n)| (n, i))
            .collect();
        let groups = groups
            .into_iter()
            .map(|(remote, names)| {
                let mut names = Vec::from_iter(names);
                names.sort_unstable_by(|a, b| ref_order(a, b));
                Group {
                    label: remote.map_or_else(|| "Local".into(), |r| format!("{r}/...")),
                    width: names.iter().map(|n| cells(short_ref(n))).max().unwrap_or(0),
                    entries: names.iter().map(|n| ids[n]).collect(),
                }
            })
            .collect();
        let refs = names
            .into_iter()
            .map(|(s, default_star)| Entry {
                name: s.to_owned(),
                default_star,
            })
            .collect();
        let original = filter
            .branch
            .as_ref()
            .map(|b| b.text(false))
            .unwrap_or_default();
        let mut m = Self {
            refs,
            groups,
            main: vec![],
            starred,
            file,
            row: 0,
            child: 0,
            main_top: 0,
            list_top: 0,
            level: Level::Main,
            original,
            query: String::new(),
            speed_search: false,
            matches: vec![],
            picked: None,
        };
        m.rebuild_main();
        Ok(m)
    }

    fn star(&self, id: usize) -> bool {
        let e = &self.refs[id];
        self.starred.get(&e.name).copied().unwrap_or(e.default_star)
    }

    fn rebuild_main(&mut self) {
        let old = self.main.get(self.row).copied();
        self.main = vec![Item::Select];
        let stars: Vec<_> = (0..self.refs.len()).filter(|&i| self.star(i)).collect();
        if !self.star(0) {
            self.main.push(Item::Ref(0));
        } // HEAD is still accessible when unstarred.
        if stars.len() > 1 {
            self.main.push(Item::Favorites);
        }
        self.main.extend(stars.into_iter().map(Item::Ref));
        if !self.groups.is_empty() {
            self.main.push(Item::Divider);
            self.main.extend((0..self.groups.len()).map(Item::Group));
        }
        self.row = old
            .and_then(|it| self.main.iter().position(|r| *r == it))
            .unwrap_or(0);
    }

    fn group(&self) -> Option<&Group> {
        if let Item::Group(i) = self.main[self.row] {
            Some(&self.groups[i])
        } else {
            None
        }
    }
    fn ref_at(&self, r: Row) -> Option<usize> {
        match r {
            Row::Main(i) => match self.main.get(i)? {
                Item::Ref(id) => Some(*id),
                _ => None,
            },
            Row::Child(i) => self.group()?.entries.get(i).copied(),
            Row::Match(i) => self.matches.get(i).copied(),
        }
    }
    fn current(&self) -> Option<Row> {
        match self.level {
            Level::Main => Some(Row::Main(self.row)),
            Level::Child => Some(Row::Child(self.child)),
            Level::Select => self.picked.map(Row::Match),
        }
    }
    fn toggle_star(&mut self, r: Row) -> Result<()> {
        let Some(id) = self.ref_at(r) else {
            return Ok(());
        };
        let e = &self.refs[id];
        let mut next = self.starred.clone();
        if self.star(id) != e.default_star {
            next.remove(&e.name);
        } else {
            next.insert(e.name.clone(), !e.default_star);
        }
        let dir = self.file.parent().unwrap();
        fs::create_dir_all(dir)?;
        let file = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(file.as_file(), &next)?;
        file.persist(&self.file)?;
        self.starred = next;
        // Update the glyph in place; moving the submenu would move the next click to a different ref.
        Ok(())
    }
    fn back(&mut self) {
        self.level = Level::Main;
        self.rebuild_main();
    }

    fn search(&mut self) {
        let q = self
            .query
            .rsplit(['|', '\n'])
            .next()
            .unwrap_or("")
            .rsplit("..")
            .next()
            .unwrap_or("")
            .trim()
            .trim_start_matches('-')
            .to_lowercase();
        self.matches = self
            .refs
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let name = if q.starts_with("refs/") {
                    &e.name
                } else {
                    short_ref(&e.name)
                };
                name.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        self.picked = (self.speed_search && !self.matches.is_empty()).then_some(0);
        self.list_top = 0;
    }
    fn select(&mut self, query: String, speed_search: bool) {
        self.level = Level::Select;
        self.query = query;
        self.speed_search = speed_search;
        self.search();
    }
    fn complete(&mut self, id: usize) {
        let tail = self.query.rfind(['|', '\n']).map_or(0, |p| p + 1);
        let start = self.query[tail..]
            .rfind("..")
            .map_or(tail, |p| tail + p + 2);
        let minus = self.query[start..].trim_start().starts_with('-');
        self.query.truncate(start);
        if minus {
            self.query.push('-');
        }
        self.query.push_str(&self.refs[id].name);
        self.search();
    }
    fn apply_query(&self) -> Result<Outcome> {
        let known = self.refs.iter().map(|e| e.name.as_str()).collect();
        Ok(Outcome::Apply(BranchFilter::parse(&self.query, &known)?))
    }
    fn hover(&mut self, r: Row) {
        match r {
            Row::Main(i) if i < self.main.len() => {
                if i != self.row || self.level != Level::Child {
                    self.child = 0;
                    self.list_top = 0;
                }
                self.row = i;
                self.level = if self.group().is_some() {
                    Level::Child
                } else {
                    Level::Main
                };
            }
            Row::Child(i) if self.level == Level::Child => self.child = i,
            Row::Match(i) if self.level == Level::Select => self.picked = Some(i),
            _ => (),
        }
    }
    fn choose(&mut self, r: Row) -> Result<Outcome> {
        if let Row::Match(_) = r {
            if let Some(id) = self.ref_at(r) {
                self.complete(id);
                return self.apply_query();
            }
        } else if let Some(id) = self.ref_at(r) {
            return Ok(Outcome::Apply(Some(BranchFilter::from(
                self.refs[id].name.as_str(),
            ))));
        } else if let Row::Main(i) = r {
            match self.main.get(i) {
                Some(Item::Select) => self.select(self.original.clone(), false),
                Some(Item::Favorites) => {
                    let mut stars = self
                        .refs
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| self.star(*i))
                        .map(|(_, e)| e.name.as_str())
                        .peekable();
                    if stars.peek().is_some() {
                        return Ok(Outcome::Apply(Some(BranchFilter::refs(stars))));
                    }
                    self.rebuild_main();
                }
                Some(Item::Group(_)) => self.hover(r),
                _ => (),
            }
        }
        Ok(Outcome::Stay)
    }
    fn move_by(&mut self, d: isize) {
        let step = |n: usize, len: usize| n.saturating_add_signed(d).min(len.saturating_sub(1));
        match self.level {
            Level::Main => {
                self.row = step(self.row, self.main.len());
                if self.main[self.row] == Item::Divider {
                    self.row = self.row.saturating_add_signed(d.signum());
                }
            }
            Level::Child => {
                if let Some(g) = self.group() {
                    self.child = step(self.child, g.entries.len());
                }
            }
            Level::Select if !self.matches.is_empty() => {
                self.picked = Some(
                    self.picked
                        .map_or(if d < 0 { self.matches.len() - 1 } else { 0 }, |i| {
                            step(i, self.matches.len())
                        }),
                );
            }
            _ => (),
        }
    }
    fn handle(&mut self, input: Input<'_>) -> Result<Outcome> {
        match input {
            Input::Click(Target::Row(r)) => return self.choose(r),
            Input::Click(Target::Star(r)) => self.toggle_star(r)?,
            Input::Click(Target::Back) => self.back(),
            Input::Click(Target::Input) => self.picked = None,
            Input::Click(Target::Apply) => return self.apply_query(),
            Input::Click(Target::Seek(Row::Main(i))) => {
                self.row = i + usize::from(self.main[i] == Item::Divider);
                self.level = Level::Main;
            }
            Input::Click(Target::Seek(r)) => self.hover(r),
            Input::Scroll(d, row) => {
                if matches!(row, Some(Row::Main(_))) {
                    self.level = Level::Main;
                }
                self.move_by(d);
            }
            Input::Paste(s) => {
                if self.level != Level::Select {
                    self.select(String::new(), false);
                }
                self.query
                    .extend(s.chars().filter(|c| !c.is_control() || *c == '\n'));
                self.search();
            }
            Input::Key(k) => {
                if self.level == Level::Select {
                    match k.code {
                        K::Esc => self.back(),
                        K::Enter if k.modifiers.contains(Mods::CONTROL) => {
                            return self.apply_query()
                        }
                        K::Enter => {
                            if let Some(r) = self.current() {
                                return self.choose(r);
                            }
                            return self.apply_query();
                        }
                        K::Tab => {
                            if let Some(id) = self.matches.get(self.picked.unwrap_or(0)).copied() {
                                self.complete(id);
                            }
                        }
                        K::Backspace => {
                            self.query.pop();
                            self.search();
                        }
                        K::Char('u') if k.modifiers.contains(Mods::CONTROL) => {
                            self.query.clear();
                            self.search();
                        }
                        K::Char(c) if !k.modifiers.intersects(Mods::ALT | Mods::CONTROL) => {
                            self.query.push(c);
                            self.search();
                        }
                        K::Down => self.move_by(1),
                        K::Up => self.move_by(-1),
                        K::PageDown => self.move_by(10),
                        K::PageUp => self.move_by(-10),
                        _ => (),
                    }
                } else {
                    match k.code {
                        K::Esc if self.level == Level::Main => return Ok(Outcome::Close),
                        K::Esc | K::Left | K::Char('h') => self.back(),
                        K::Down | K::Char('j') => self.move_by(1),
                        K::Up | K::Char('k') => self.move_by(-1),
                        K::PageDown => self.move_by(10),
                        K::PageUp => self.move_by(-10),
                        K::Home => self.move_by(isize::MIN),
                        K::End => self.move_by(isize::MAX),
                        K::Right | K::Char('l')
                            if self.level == Level::Main && self.group().is_some() =>
                        {
                            self.hover(Row::Main(self.row))
                        }
                        K::Char('l') => (),
                        K::Enter => return self.choose(self.current().unwrap()),
                        K::Char(' ') => self.toggle_star(self.current().unwrap())?,
                        K::Char('/') => self.select(String::new(), true),
                        K::Char(ch) if !k.modifiers.intersects(Mods::ALT | Mods::CONTROL) => {
                            self.select(ch.to_string(), true)
                        }
                        _ => (),
                    }
                }
            }
        }
        Ok(Outcome::Stay)
    }
}

impl LogPanel {
    pub(crate) fn hover_menu_at(&mut self, x: u16, y: u16) -> bool {
        if self.view_menu.is_none() && self.branch_menu.is_none() {
            return false;
        }
        match self
            .hits
            .iter()
            .rev()
            .find(|(r, _)| inside(*r, x, y))
            .map(|(_, h)| h)
        {
            Some(Hit::Dropdown(DropdownHit::Branch(Target::Row(row) | Target::Star(row)))) => {
                let Some(m) = &mut self.branch_menu else {
                    return false;
                };
                let old = (m.level, m.row, m.child, m.picked);
                m.hover(*row);
                old != (m.level, m.row, m.child, m.picked)
            }
            Some(Hit::ViewMenu(h)) if self.view_menu.is_some() => {
                self.hover_view_menu(*h);
                true
            }
            _ => false,
        }
    }
    pub(super) fn branch_input(&mut self, input: Input<'_>, c: &Context) -> Result<()> {
        let Some(menu) = &mut self.branch_menu else {
            return Ok(());
        };
        match menu.handle(input)? {
            Outcome::Close => self.discard_editor(),
            Outcome::Apply(filter) => {
                self.filter.branch = filter;
                self.discard_editor();
                self.reload(c, false)?;
            }
            Outcome::Stay => (),
        }
        Ok(())
    }
}
