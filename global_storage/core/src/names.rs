//! Item names, so players find "алмаз", "diamond" or "diamond_sword" alike.
//!
//! `data/names.tsv` is made from the game's language files (26.3): the item id, its Ukrainian
//! name, its English name. Items it doesn't know (other mods') are found by their id.

use std::collections::HashMap;

const TABLE: &str = include_str!("../data/names.tsv");

pub struct Names {
    map: HashMap<&'static str, (&'static str, &'static str)>,
}

impl Default for Names {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Names {
    pub fn builtin() -> Self {
        let map = TABLE
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                Some((f.next()?, (f.next()?, f.next()?)))
            })
            .collect();
        Self { map }
    }

    /// The Ukrainian name, or the id made readable (`create:brass_ingot` → `Brass Ingot`).
    pub fn display(&self, id: &str) -> String {
        match self.map.get(id) {
            Some((uk, _)) => (*uk).to_owned(),
            None => readable(id),
        }
    }

    /// Everything a search looks in, lowercase: the id, its path with spaces, both names.
    pub fn haystack(&self, id: &str) -> String {
        let path = id.split_once(':').map_or(id, |(_, p)| p);
        let mut out = format!("{id} {}", path.replace('_', " "));
        if let Some((uk, en)) = self.map.get(id) {
            out.push(' ');
            out.push_str(uk);
            out.push(' ');
            out.push_str(en);
        }
        out.to_lowercase()
    }
}

fn readable(id: &str) -> String {
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    path.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
