//! Global Storage for Horizon: everything that doesn't need the game.
//!
//! - [`config`]: the settings file.
//! - [`store`]: the storage itself, one entry per kind of item (item and components) with a
//!   count that has no stack limit.
//! - [`names`]: item names in Ukrainian and English, for search and sorting.
//! - [`creative`]: the creative inventory's tabs and order.
//! - [`view`]: search, creative tabs, sorting and pages.
//! - [`gui`]: the chest menu's layout (for vanilla clients), its buttons and what each click means.
//! - [`term`]: the terminal's messages between the server and the Horizon client mod.
//!
//! The game's side (`glue.rs`) turns entries into item stacks and moves them in and out of
//! the players' inventories.

pub mod config;
pub mod creative;
pub mod gui;
pub mod names;
pub mod store;
pub mod term;
pub mod view;

#[cfg(test)]
mod tests;

/// `12345678` as `12 345 678`.
pub fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// A short count for a menu line: `950`, `12.3 тис.`, `4.5 млн`.
pub fn short(n: u64) -> String {
    match n {
        0..10_000 => group(n),
        10_000..1_000_000 => format!("{:.1} тис.", n as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} млн", n as f64 / 1e6),
        _ => format!("{:.1} млрд", n as f64 / 1e9),
    }
}

/// How a count splits into stacks: `"3 ст. + 12"`, `"5 ст."`, `"12"`.
pub fn stacks(n: u64, max: u32) -> String {
    let max = u64::from(max.max(1));
    if max == 1 || n < max {
        return group(n);
    }
    let (full, rest) = (n / max, n % max);
    if rest == 0 {
        format!("{} ст.", group(full))
    } else {
        format!("{} ст. + {}", group(full), rest)
    }
}
