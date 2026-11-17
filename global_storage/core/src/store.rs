//! The storage: one entry per kind of item, each with a count of any size.
//!
//! An entry is found by its `key`, a text that is equal for two stacks exactly when they stack
//! in the game (the same item and the same components); the game's side makes it. `T` is what
//! the game needs to hand the item back (its stack); the tests use plain text.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry<T> {
    /// Equal for stacks that stack together.
    pub key: String,
    /// The item's id, `minecraft:diamond`.
    pub id: String,
    /// The stack's own name (an anvil's), lowercase; empty when it has none.
    pub label: String,
    /// One of the item, as it goes back out.
    pub item: T,
    pub count: u64,
}

#[derive(Clone, Debug)]
pub struct Store<T> {
    entries: Vec<Entry<T>>,
    index: HashMap<String, usize>,
    /// Changed since the last save.
    pub dirty: bool,
    /// Grows with every change, so open menus know to redraw.
    pub version: u64,
    /// The file could not be read: nothing goes in or out and nothing is saved over it, so an
    /// admin can mend the file without a storage started empty having been used meanwhile.
    pub locked: bool,
    /// The keys' lengths added up: about the size of the saved file.
    key_bytes: usize,
}

impl<T> Default for Store<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
            dirty: false,
            version: 0,
            locked: false,
            key_bytes: 0,
        }
    }
}

/// Why something didn't go in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The storage holds `max_types` kinds already and this is a new one.
    Full,
    /// The count would pass `u64::MAX`.
    Overflow,
    /// One of the item is bigger than the storage takes (a book full of text, a full shulker).
    TooBig,
    /// The storage's file would grow past its size limit.
    NoRoom,
    /// The storage's file is broken: the storage is closed until an admin mends it.
    Locked,
}

impl<T> Store<T> {
    pub fn entries(&self) -> &[Entry<T>] {
        &self.entries
    }

    pub fn get(&self, key: &str) -> Option<&Entry<T>> {
        self.index.get(key).map(|&i| &self.entries[i])
    }

    pub fn count(&self, key: &str) -> u64 {
        self.get(key).map_or(0, |e| e.count)
    }

    /// How many kinds of item.
    pub fn types(&self) -> usize {
        self.entries.len()
    }

    /// How many items in all.
    pub fn total(&self) -> u128 {
        self.entries.iter().map(|e| u128::from(e.count)).sum()
    }

    /// About how many bytes the storage's file takes (its keys, the items' saved forms).
    pub fn key_bytes(&self) -> usize {
        self.key_bytes
    }

    /// Whether a new kind with this `key` fits: one item at most `max_item` bytes and the
    /// whole storage at most `max_total` (0 for no limit). A kind already there always fits.
    pub fn fits(&self, key: &str, max_item: usize, max_total: usize) -> Result<(), Refused> {
        if self.locked {
            return Err(Refused::Locked);
        }
        if self.get(key).is_some() {
            return Ok(());
        }
        if max_item > 0 && key.len() > max_item {
            return Err(Refused::TooBig);
        }
        if max_total > 0 && self.key_bytes + key.len() > max_total {
            return Err(Refused::NoRoom);
        }
        Ok(())
    }

    /// Whether `add` would take `n` of `key` (with `max_types` kinds at most, 0 for any).
    pub fn accepts(&self, key: &str, n: u64, max_types: usize) -> Result<(), Refused> {
        if self.locked {
            return Err(Refused::Locked);
        }
        match self.get(key) {
            Some(e) => e.count.checked_add(n).map(|_| ()).ok_or(Refused::Overflow),
            None if max_types > 0 && self.entries.len() >= max_types => Err(Refused::Full),
            None => Ok(()),
        }
    }

    /// Puts `n` of a kind in; `item` makes the entry when the kind is new.
    pub fn add(
        &mut self,
        key: &str,
        n: u64,
        max_types: usize,
        item: impl FnOnce() -> (String, String, T),
    ) -> Result<(), Refused> {
        if n == 0 {
            return Ok(());
        }
        self.accepts(key, n, max_types)?;
        match self.index.get(key) {
            Some(&i) => self.entries[i].count += n,
            None => {
                let (id, label, item) = item();
                self.key_bytes += key.len();
                self.index.insert(key.to_owned(), self.entries.len());
                self.entries.push(Entry {
                    key: key.to_owned(),
                    id,
                    label,
                    item,
                    count: n,
                });
            }
        }
        self.changed();
        Ok(())
    }

    /// Takes up to `n` of `key` out: how many it took. An emptied entry goes away.
    pub fn take(&mut self, key: &str, n: u64) -> u64 {
        if self.locked {
            return 0;
        }
        let Some(&i) = self.index.get(key) else {
            return 0;
        };
        let taken = n.min(self.entries[i].count);
        if taken == 0 {
            return 0;
        }
        self.entries[i].count -= taken;
        if self.entries[i].count == 0 {
            self.index.remove(key);
            self.key_bytes -= key.len();
            self.entries.swap_remove(i);
            if let Some(moved) = self.entries.get(i) {
                self.index.insert(moved.key.clone(), i);
            }
        }
        self.changed();
        taken
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.version += 1;
    }
}
