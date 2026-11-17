//! The terminal: the storage as the Horizon client mod (serverkeys) draws it over the chest,
//! with its own search box and scrolling grid. The server tells the client what the storage
//! holds; the client filters and sorts it itself, so typing in the search costs the server
//! nothing.
//!
//! Server to client, channel [`TO_CLIENT`]: records, each a line of fields, joined in messages
//! of at most [`MAX_MESSAGE`] bytes. Fields are split by [`FIELD`], records by [`RECORD`] (two
//! control characters no name or item data holds):
//!
//! - `open`, title: the terminal opens; forget the old list.
//! - `add`, id, count, name, search text, item (SNBT): a kind the client hasn't been told of.
//! - `set`, id, count: how many there are now.
//! - `del`, id: none left.
//! - `close`: the terminal closed.
//!
//! Client to server, channel [`TO_SERVER`]: one action a message:
//!
//! - `take`, id, `stack` | `one` | `fill`: take that kind out.
//! - `deposit`, `all` | `known`: the deposit button (everything, or kinds already in).

use crate::gui::Take;
use std::collections::HashMap;

pub const TO_CLIENT: &str = "global_storage:term";
pub const TO_SERVER: &str = "global_storage:act";
pub const FIELD: char = '\u{1f}';
pub const RECORD: char = '\u{1e}';
/// Under the mod messages' 30 000 bytes.
pub const MAX_MESSAGE: usize = 28_000;
/// The serverkeys hello that says the client draws the terminal (`hello:4` and later).
pub const TERMINAL_HELLO: u32 = 4;

/// A field as sent: the separators can't appear in it.
pub fn clean(s: &str) -> String {
    s.replace([FIELD, RECORD], " ")
}

/// Records packed in messages of at most `max` bytes (a record bigger than that alone).
pub fn pack(records: &[String], max: usize) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    for r in records {
        let extra = r.len() + usize::from(!cur.is_empty());
        if !cur.is_empty() && cur.len() + extra > max {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.extend_from_slice(RECORD.to_string().as_bytes());
        }
        cur.extend_from_slice(r.as_bytes());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn record(fields: &[&str]) -> String {
    fields.join(&FIELD.to_string())
}

/// Numbers for the kinds of one storage, stable while the server runs.
#[derive(Default)]
pub struct Ids {
    by_key: HashMap<String, u32>,
    by_id: HashMap<u32, String>,
    next: u32,
}

impl Ids {
    pub fn of(&mut self, key: &str) -> u32 {
        if let Some(&id) = self.by_key.get(key) {
            return id;
        }
        self.next += 1;
        self.by_key.insert(key.to_owned(), self.next);
        self.by_id.insert(self.next, key.to_owned());
        self.next
    }
    pub fn key(&self, id: u32) -> Option<&str> {
        self.by_id.get(&id).map(String::as_str)
    }
}

/// What changed between what a client was told (`sent`, updated in place) and the storage now
/// (`now`: id and count of every kind): kinds to add, counts to set, kinds gone.
pub fn diff(
    sent: &mut HashMap<u32, u64>,
    now: impl IntoIterator<Item = (u32, u64)>,
) -> (Vec<u32>, Vec<(u32, u64)>, Vec<u32>) {
    let (mut add, mut set) = (Vec::new(), Vec::new());
    let mut seen = std::collections::HashSet::new();
    for (id, count) in now {
        seen.insert(id);
        match sent.insert(id, count) {
            None => add.push(id),
            Some(old) if old != count => set.push((id, count)),
            Some(_) => {}
        }
    }
    let mut del: Vec<u32> = sent.keys().filter(|id| !seen.contains(id)).copied().collect();
    del.sort_unstable();
    for id in &del {
        sent.remove(id);
    }
    (add, set, del)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Take(u32, Take),
    /// The deposit button: `true` for kinds already in only.
    Deposit(bool),
}

pub fn parse_act(text: &str) -> Option<Act> {
    let f: Vec<&str> = text.split(FIELD).collect();
    match f.as_slice() {
        ["take", id, how] => {
            let how = match *how {
                "stack" => Take::Stack,
                "one" => Take::One,
                "fill" => Take::Fill,
                _ => return None,
            };
            Some(Act::Take(id.parse().ok()?, how))
        }
        ["deposit", "all"] => Some(Act::Deposit(false)),
        ["deposit", "known"] => Some(Act::Deposit(true)),
        _ => None,
    }
}

/// The version a serverkeys hello (`hello:4`) says.
pub fn hello_version(text: &str) -> Option<u32> {
    text.strip_prefix("hello:")?.parse().ok()
}
