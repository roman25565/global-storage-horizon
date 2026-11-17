//! What a player sees: the entries that match their search, in their order, a page at a time.

use crate::{
    creative::{Creative, Tab},
    names::Names,
    store::{Entry, Store},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sort {
    /// As the creative inventory orders them.
    #[default]
    Creative,
    /// Most first.
    Count,
    /// By name, А to Я.
    Name,
    /// By mod, then id.
    Id,
}

impl Sort {
    pub fn next(self) -> Self {
        match self {
            Sort::Creative => Sort::Count,
            Sort::Count => Sort::Name,
            Sort::Name => Sort::Id,
            Sort::Id => Sort::Creative,
        }
    }
    pub fn prev(self) -> Self {
        self.next().next().next()
    }
    pub const ALL: [Sort; 4] = [Sort::Creative, Sort::Count, Sort::Name, Sort::Id];
    pub fn label(self) -> &'static str {
        match self {
            Sort::Creative => "як у креативі",
            Sort::Count => "за кількістю",
            Sort::Name => "за назвою",
            Sort::Id => "за модом та id",
        }
    }
}

/// A search typed by a player, ready to match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    /// Each must be in the item's names, id or own name.
    words: Vec<String>,
    /// `@create`: the id's namespace starts with it.
    mods: Vec<String>,
}

fn fold(s: &str) -> String {
    s.to_lowercase().replace(['’', 'ʼ', '`'], "'")
}

impl Query {
    pub fn parse(text: &str) -> Self {
        let mut q = Query::default();
        for word in fold(text).split_whitespace() {
            match word.strip_prefix('@') {
                Some("") => {}
                Some(m) => q.mods.push(m.to_owned()),
                None => q.words.push(word.to_owned()),
            }
        }
        q
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.mods.is_empty()
    }

    /// Whether the item `id` with its own `label` (lowercase) matches; `haystack` is
    /// [`Names::haystack`] of the id.
    pub fn matches(&self, id: &str, label: &str, haystack: &str) -> bool {
        let namespace = id.split_once(':').map_or("minecraft", |(n, _)| n);
        self.mods.iter().all(|m| namespace.starts_with(m.as_str()))
            && self.words.iter().all(|w| {
                haystack.contains(w.as_str()) || fold(label).contains(w.as_str())
            })
    }
}

/// What a player looks at: their search, the creative tab they picked and their order.
pub struct Look<'a> {
    pub query: &'a Query,
    pub tab: Tab,
    pub sort: Sort,
}

/// The indexes (into `store.entries()`) a player sees, in order.
pub fn select<T>(store: &Store<T>, names: &Names, creative: &Creative, look: &Look) -> Vec<usize> {
    let (query, sort) = (look.query, look.sort);
    let entries = store.entries();
    let mut shown: Vec<(usize, String)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| creative.shows(look.tab, &e.id))
        .filter(|(_, e)| query.is_empty() || query.matches(&e.id, &e.label, &names.haystack(&e.id)))
        .map(|(i, e)| (i, sort_name(names, e)))
        .collect();
    shown.sort_by(|(a, an), (b, bn)| {
        let (a, b) = (&entries[*a], &entries[*b]);
        let by_name = an.cmp(bn).then_with(|| a.key.cmp(&b.key));
        match sort {
            Sort::Creative => creative
                .order(&a.id)
                .cmp(&creative.order(&b.id))
                .then_with(|| a.id.cmp(&b.id))
                .then(by_name),
            Sort::Count => b.count.cmp(&a.count).then(by_name),
            Sort::Name => by_name,
            Sort::Id => a.id.cmp(&b.id).then(by_name),
        }
    });
    shown.into_iter().map(|(i, _)| i).collect()
}

fn sort_name<T>(names: &Names, e: &Entry<T>) -> String {
    if e.label.is_empty() {
        fold(&names.display(&e.id))
    } else {
        e.label.clone()
    }
}

/// How many pages `n` entries fill, `per_page` a page; at least one.
pub fn pages(n: usize, per_page: usize) -> usize {
    n.div_ceil(per_page.max(1)).max(1)
}
