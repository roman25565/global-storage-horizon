//! The storage's chest menu, for players whose client has no terminal (a vanilla client):
//! five rows of items (a page), the buttons on the sixth.
//!
//! ```text
//!  0 ..  8   items
//!  ...
//! 36 .. 44   items
//! 45 ◀  46 ▒  47 search  48 sort  49 info  50 deposit  51 ▒  52 ▒  53 ▶
//! ```
//! Below the menu is the player's own inventory: a click there puts that stack in.
//! Players with the Horizon client mod see the terminal instead (`term`), drawn over the same
//! chest, whose own slots then stay empty.

use crate::view::Sort;

pub const ROWS: u8 = 6;
pub const SIZE: usize = 54;
/// The first slot of the items.
pub const FIRST: usize = 0;
pub const PER_PAGE: usize = 45;

pub const PREV: usize = 45;
pub const SEARCH: usize = 47;
pub const SORT: usize = 48;
pub const INFO: usize = 49;
pub const DEPOSIT: usize = 50;
pub const NEXT: usize = 53;

/// Text colours, as Minecraft names them.
pub const GOLD: &str = "gold";
pub const GRAY: &str = "gray";
pub const DARK_GRAY: &str = "dark_gray";
pub const YELLOW: &str = "yellow";
pub const GREEN: &str = "green";
pub const AQUA: &str = "aqua";
pub const WHITE: &str = "white";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Prev,
    Next,
    Search,
    Sort,
    Info,
    Deposit,
}

/// What one slot of the menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// The entry at this index of the store.
    Entry(usize),
    Button(Button),
    /// A pane in the button row.
    Filler,
    /// Nothing: past the last item.
    Empty,
}

/// The menu's 54 slots for page `page` (from 0) of `shown`.
pub fn layout(shown: &[usize], page: usize) -> Vec<Slot> {
    let pages = crate::view::pages(shown.len(), PER_PAGE);
    let page = page.min(pages - 1);
    let mut slots = vec![Slot::Filler; SIZE];
    let mut items = shown.iter().skip(page * PER_PAGE).take(PER_PAGE);
    for slot in &mut slots[FIRST..FIRST + PER_PAGE] {
        *slot = items.next().map_or(Slot::Empty, |&i| Slot::Entry(i));
    }
    if page > 0 {
        slots[PREV] = Slot::Button(Button::Prev);
    }
    if page + 1 < pages {
        slots[NEXT] = Slot::Button(Button::Next);
    }
    slots[SEARCH] = Slot::Button(Button::Search);
    slots[SORT] = Slot::Button(Button::Sort);
    slots[DEPOSIT] = Slot::Button(Button::Deposit);
    slots[INFO] = Slot::Button(Button::Info);
    slots
}

/// How many to take out of the storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// A full stack (left click).
    Stack,
    /// One (right click).
    One,
    /// As many as fit in the inventory (shift-click).
    Fill,
    /// A full stack into this hotbar slot, 0 to 8 (the number keys).
    Hotbar(usize),
}

/// What a click in the menu asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Take some of what this menu slot shows (if it shows an item).
    Take(usize, Take),
    /// A button (if this slot is one); `alt` for a right or shift click.
    Press(usize, bool),
    /// Put the stack in this inventory slot (0..9 the hotbar) in.
    Deposit(usize),
    /// Put every stack like the one in this inventory slot in (double click).
    DepositAll(usize),
    None,
}

/// A click's meaning. `slot`, `button` and `mode` are as the client sends them
/// (`ServerboundContainerClickPacket`).
pub fn action(slot: i32, button: i32, mode: i32) -> Action {
    let Ok(s) = usize::try_from(slot) else {
        return Action::None;
    };
    if (FIRST..FIRST + PER_PAGE).contains(&s) {
        let take = match (mode, button) {
            (0, 0) => Take::Stack,
            (0, 1) => Take::One,
            (1, _) => Take::Fill,
            (2, n @ 0..=8) => Take::Hotbar(n as usize),
            _ => return Action::None,
        };
        return Action::Take(s, take);
    }
    if s < SIZE {
        return match (mode, button) {
            (0, b) => Action::Press(s, b == 1),
            (1, _) => Action::Press(s, true),
            _ => Action::None,
        };
    }
    let Some(inv) = inventory_slot(s) else {
        return Action::None;
    };
    match mode {
        0 | 1 => Action::Deposit(inv),
        6 => Action::DepositAll(inv),
        _ => Action::None,
    }
}

/// The inventory slot (0..9 the hotbar, 9..36 the rest) of menu slot `s`: after the menu's
/// own come the inventory's 27, then the hotbar's 9.
pub fn inventory_slot(s: usize) -> Option<usize> {
    match s.checked_sub(SIZE)? {
        n @ 0..27 => Some(n + 9),
        n @ 27..36 => Some(n - 27),
        _ => None,
    }
}

/// A line of an item's lore: text and colour.
pub type Line = (String, &'static str);

/// The lines added under an item of the storage: how many, and how to take them.
pub fn entry_lore(count: u64, max_stack: u32) -> Vec<Line> {
    let mut lines = vec![(format!("У сховищі: {}", crate::group(count)), GOLD)];
    if max_stack > 1 && count >= u64::from(max_stack) {
        lines.push((format!("({})", crate::stacks(count, max_stack)), GRAY));
    }
    lines.push(("ЛКМ — стак · ПКМ — 1 шт.".into(), DARK_GRAY));
    lines.push(("Shift+клік — скільки влізе · 1-9 — у слот".into(), DARK_GRAY));
    lines
}

/// What the buttons tell.
pub struct Status<'a> {
    pub page: usize,
    pub pages: usize,
    pub types: usize,
    pub total: u128,
    pub shown: usize,
    pub query: &'a str,
    pub sort: Sort,
    pub deposit_hotbar: bool,
    pub shared: bool,
}

/// A button's item, name, name colour and lore.
pub fn button(b: Button, st: &Status) -> (&'static str, String, &'static str, Vec<Line>) {
    match b {
        Button::Prev => (
            "minecraft:arrow",
            "◀ Попередня сторінка".into(),
            GOLD,
            vec![
                (format!("Сторінка {} з {}", st.page + 1, st.pages), GRAY),
                ("ПКМ — на першу".into(), DARK_GRAY),
            ],
        ),
        Button::Next => (
            "minecraft:arrow",
            "Наступна сторінка ▶".into(),
            GOLD,
            vec![
                (format!("Сторінка {} з {}", st.page + 1, st.pages), GRAY),
                ("ПКМ — на останню".into(), DARK_GRAY),
            ],
        ),
        Button::Search => {
            let mut lore = Vec::new();
            if st.query.is_empty() {
                lore.push(("Зараз: без пошуку".into(), GRAY));
            } else {
                lore.push((format!("Зараз: «{}» ({} знайдено)", st.query, st.shown), YELLOW));
            }
            lore.push(("ЛКМ — ввести пошук".into(), GREEN));
            if !st.query.is_empty() {
                lore.push(("ПКМ — скинути пошук".into(), GREEN));
            }
            lore.push(("Назва українською чи англійською, id,".into(), DARK_GRAY));
            lore.push(("@мод — лише предмети мода (@create)".into(), DARK_GRAY));
            lore.push(("Або в чаті: /gs <текст>".into(), DARK_GRAY));
            ("minecraft:spyglass", "Пошук".into(), GOLD, lore)
        }
        Button::Sort => {
            let mut lore = Vec::new();
            for s in Sort::ALL {
                if s == st.sort {
                    lore.push((format!("▶ {}", s.label()), YELLOW));
                } else {
                    lore.push((format!("  {}", s.label()), GRAY));
                }
            }
            lore.push(("ЛКМ — далі · ПКМ — назад".into(), GREEN));
            ("minecraft:hopper", "Сортування".into(), GOLD, lore)
        }
        Button::Info => {
            let lore = vec![
                (format!("Видів предметів: {}", crate::group(st.types as u64)), GRAY),
                (format!("Предметів усього: {}", group128(st.total)), GRAY),
                (format!("Сторінка {} з {}", st.page + 1, st.pages), GRAY),
                (
                    if st.shared {
                        "Спільне для всіх гравців".into()
                    } else {
                        "Ваше особисте сховище".into()
                    },
                    AQUA,
                ),
                (String::new(), GRAY),
                ("Клік по своєму предмету внизу — покласти".into(), DARK_GRAY),
                ("Подвійний клік — покласти всі такі".into(), DARK_GRAY),
                ("Ліміту на стак немає".into(), DARK_GRAY),
            ];
            ("minecraft:book", "Сховище".into(), GOLD, lore)
        }
        Button::Deposit => {
            let from = if st.deposit_hotbar {
                "з усього інвентаря"
            } else {
                "з інвентаря (без хотбару)"
            };
            (
                "minecraft:chest",
                "Покласти все".into(),
                GOLD,
                vec![
                    (format!("ЛКМ — усі предмети {from}"), GREEN),
                    ("ПКМ — лише ті, що вже є в сховищі".into(), GREEN),
                ],
            )
        }
    }
}

fn group128(n: u128) -> String {
    match u64::try_from(n) {
        Ok(n) => crate::group(n),
        Err(_) => n.to_string(),
    }
}

/// The menu's title, with the search when there is one. The tab isn't in it: the title only
/// changes by opening the menu again, which would move the player's mouse.
pub fn title(base: &str, query: &str) -> String {
    let mut out = base.to_owned();
    if !query.is_empty() {
        let q: String = query.chars().take(20).collect();
        let more = if query.chars().count() > 20 { "…" } else { "" };
        out.push_str(&format!(": {q}{more}"));
    }
    out
}
