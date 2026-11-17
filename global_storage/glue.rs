// Global Storage in the server's game, added as `game::global_storage_glue`: the /gs command,
// the chest menu, moving stacks between the players' inventories and the storage, and the
// storage's files. The rules (search, sorting, the menu's layout) live in the mod's crate,
// `global_storage_core`.
//
// Everything runs on the server's thread. The state is behind a mutex taken with `try_lock`:
// should a game call ever come back into one of our hooks, that call does nothing instead of
// waiting on itself.

use crate::game::{
    Game,
    native_server::{NativeCommand, NativeServerPlayer},
};
use global_storage_core::{
    config::{self, Config, Mode},
    creative::{Creative, Tab},
    gui::{self, Action, Button, Slot, Take},
    names::Names,
    store::{Refused, Store},
    term::{self, Act, Ids},
    view::{self, Look, Query, Sort},
};
use horizon_client::{
    items::ItemStack,
    lan::data::component_nbt,
    text::{self, Text},
};
use horizon_items::{item_data::canon_stack, item_nbt};
use horizon_world::nbt::{self, NamedTag, Tag};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, TryLockError},
    time::{Duration, Instant},
};

/// Where the settings are, from the server's folder.
const CONFIG: &str = "config/global_storage/global-storage.toml";
/// The storages' folder in the world.
const FOLDER: &str = "global_storage";
/// The commands' words. Horizon allows lowercase words only.
const WORDS: [&str; 2] = ["gs", "storage"];
/// The search dialog's text runs `/gs =<text>`: the `=` keeps an emptied box a valid command.
const MARK: char = '=';
/// A client is taken to have the terminal this long after its last hello (it says one every
/// 10 seconds).
const TERMINAL_FRESH: Duration = Duration::from_secs(35);
/// Terminal messages sent to one player in a tick, at most (the game allows 32).
const SENDS_PER_TICK: usize = 24;

struct View {
    owner: String,
    window: Option<i32>,
    query: String,
    sort: Sort,
    page: usize,
    /// What each of the 54 slots shows, as last sent.
    slots: Vec<Slot>,
    /// The key of the entry in each of the 54 slots, as last sent: a click takes what the
    /// player saw, even if the order changed since.
    keys: Vec<Option<String>>,
    /// The kind a click below the menu last put in, and when: a double click sends a plain
    /// click (which puts the stack in and empties the slot) and then "pick up all" on the same,
    /// now empty, slot.
    last_deposit: Option<(String, Instant)>,
    /// Drawn as a terminal by the player's client (the chest's own slots stay empty).
    terminal: bool,
    /// The terminal's kinds and counts, as last told.
    sent: HashMap<u32, u64>,
}

struct State {
    config: Config,
    names: Names,
    creative: Creative,
    world: Option<PathBuf>,
    stores: HashMap<String, Store<ItemStack>>,
    views: HashMap<u128, View>,
    /// A player's sort, kept between openings.
    sorts: HashMap<u128, Sort>,
    /// Asked for (a command, a button), opened at the next tick: player and search.
    pending: Vec<(u128, String)>,
    last_save: Instant,
    /// Each storage's numbers for its kinds, as the terminals know them.
    ids: HashMap<String, Ids>,
    /// Players whose client draws the terminal: their last hello.
    terminals: HashMap<u128, Instant>,
    /// Terminal messages waiting to be sent, a few a tick.
    outbox: HashMap<u128, VecDeque<Vec<u8>>>,
    /// Players who moved items in or out since the storage was last saved: their files are
    /// saved with it, so a crash rolls both back to the same moment (no copy, no loss).
    touched: HashSet<u128>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

const LOCKED: &str = "Сховище тимчасово закрите: його файл пошкоджено. Скажіть адміну.";

/// The settings, written with the defaults when the file is missing.
fn load_config() -> Config {
    let path = Path::new(CONFIG);
    match std::fs::read_to_string(path) {
        Ok(text) => match config::parse(&text) {
            Ok(c) => c,
            Err(e) => {
                log::error!(target: "horizon::global_storage", "Global Storage: {CONFIG}: {e}; using the defaults");
                Config::default()
            }
        },
        Err(_) => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(path, config::DEFAULT) {
                log::warn!(target: "horizon::global_storage", "Global Storage: can't write {CONFIG}: {e}");
            }
            Config::default()
        }
    }
}

/// The state, or None when a hook of ours is already running (see the top).
fn state() -> Option<MutexGuard<'static, Option<State>>> {
    match STATE.try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

fn init(slot: &mut Option<State>) -> &mut State {
    slot.get_or_insert_with(|| {
        let config = load_config();
        log::info!(
            target: "horizon::global_storage",
            "Global Storage: {} storage, /gs to open",
            match config.mode {
                Mode::Global => "one shared",
                Mode::PerPlayer => "a per-player",
            }
        );
        State {
            config,
            names: Names::builtin(),
            creative: Creative::builtin(),
            world: None,
            stores: HashMap::new(),
            views: HashMap::new(),
            sorts: HashMap::new(),
            pending: Vec::new(),
            last_save: Instant::now(),
        touched: HashSet::new(),
            ids: HashMap::new(),
            terminals: HashMap::new(),
            outbox: HashMap::new(),
        }
    })
}

// ---- Commands ---------------------------------------------------------------------------

pub fn commands(commands: &mut Vec<NativeCommand>) {
    let level = load_config().permission;
    for word in WORDS {
        let usage = format!("{word} [search:text]");
        commands.push(if level == 0 {
            NativeCommand::new(&usage)
        } else {
            NativeCommand::op(&usage, level)
        });
    }
}

pub(in crate::game) fn perform(
    game: &mut Game<'_>,
    player: &NativeServerPlayer,
    line: &str,
    ran: &mut bool,
) {
    let line = line.trim();
    let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    if !WORDS.contains(&word) {
        return;
    }
    *ran = true;
    let Some(mut guard) = state() else {
        return;
    };
    let st = init(&mut *guard);
    if player.permission < st.config.permission {
        game.native_feedback(
            Text::literal("Сховище доступне лише операторам.").with_color(text::RED),
        );
        return;
    }
    let query = rest.trim().trim_start_matches(MARK).trim().to_owned();
    st.pending.retain(|(p, _)| *p != player.uuid);
    st.pending.push((player.uuid, query));
}

/// The serverkeys client mod's channel: `<server mod>:<action>` for a key a player pressed.
const KEYS_CHANNEL: &str = "serverkeys:action";

/// A key of the serverkeys client mod: "open storage" opens it as `/gs` does. Heard in the
/// tick after it arrives, with no player's state swapped in.
pub(in crate::game) fn net_from_client(
    game: &mut Game<'_>,
    player: u128,
    channel: &str,
    data: &[u8],
) {
    if channel != KEYS_CHANNEL && channel != term::TO_SERVER {
        return;
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if channel == KEYS_CHANNEL
        && term::hello_version(text).is_none()
        && text != "global_storage:open"
    {
        return;
    }
    let Some(mut guard) = state() else {
        return;
    };
    let st = init(&mut *guard);
    if channel == term::TO_SERVER {
        act(game, st, player, text);
        return;
    }
    // The serverkeys client says hello every 10 seconds: from version 4 it draws the terminal.
    if let Some(v) = term::hello_version(text) {
        if v >= term::TERMINAL_HELLO {
            st.terminals.insert(player, Instant::now());
        } else {
            st.terminals.remove(&player);
        }
        return;
    }
    if st.config.permission > 0
        && !game
            .native_server_players()
            .iter()
            .any(|p| p.uuid == player && p.permission >= st.config.permission)
    {
        game.native_tell(
            player,
            Text::literal("Сховище доступне лише операторам.").with_color(text::RED),
        );
        return;
    }
    if !st.pending.iter().any(|(p, _)| *p == player) {
        st.pending.push((player, String::new()));
    }
}

/// An action from a player's terminal: take a kind out, or the deposit button.
fn act(game: &mut Game<'_>, st: &mut State, player: u128, text: &str) {
    let Some(view) = st.views.get(&player).filter(|v| v.terminal) else {
        return;
    };
    let owner = view.owner.clone();
    let config = st.config.clone();
    st.touched.insert(player);
    match term::parse_act(text) {
        Some(Act::Take(id, how)) => {
            let Some(k) = st
                .ids
                .get(&owner)
                .and_then(|ids| ids.key(id))
                .map(str::to_owned)
            else {
                return;
            };
            let store = store(st, game, &owner);
            if let Some((msg, color)) = take(game, store, player, &k, how) {
                game.native_tell(player, Text::literal(msg).with_color(color));
            }
        }
        Some(Act::Deposit(known)) => deposit_button(game, st, player, &owner, &config, known),
        None => return,
    }
    refresh(game, st, &owner);
}

/// The deposit button: everything in the inventory (the hotbar only if the settings say so),
/// or with `known` only the kinds the storage holds already.
fn deposit_button(
    game: &mut Game<'_>,
    st: &mut State,
    uuid: u128,
    owner: &str,
    config: &Config,
    known: bool,
) {
    let slots: Vec<usize> = (if config.deposit_hotbar { 0..36 } else { 9..36 }).collect();
    let store = store(st, game, owner);
    let m = if known {
        let keys: HashSet<String> = store.entries().iter().map(|e| e.key.clone()).collect();
        deposit(game, store, config, uuid, &slots, |k| keys.contains(k))
    } else {
        deposit(game, store, config, uuid, &slots, |_| true)
    };
    if m.items == 0 && m.banned == 0 && !m.full && !m.too_big && !m.no_room && !m.locked {
        game.native_tell(uuid, Text::literal("Нічого класти.").with_color(text::YELLOW));
    }
    tell_moved(game, uuid, &m, false);
}

// ---- The storages and their files -------------------------------------------------------

fn owner(config: &Config, uuid: u128) -> String {
    match config.mode {
        Mode::Global => "global".into(),
        Mode::PerPlayer => format!("{uuid:032x}"),
    }
}

/// One of the item, as it is kept: the count is the entry's.
fn proto(stack: &ItemStack) -> ItemStack {
    ItemStack {
        count: 1,
        ..stack.clone()
    }
}

/// Equal for two stacks exactly when they stack (`ItemStack.isSameItemSameComponents`), as
/// the stack's saved form.
fn key(proto: &ItemStack) -> String {
    canon_stack(Some(proto))
}

fn label(proto: &ItemStack) -> String {
    proto
        .data()
        .and_then(|d| d.custom_name_plain())
        .unwrap_or_default()
        .to_lowercase()
}

fn new_entry(p: &ItemStack) -> (String, String, ItemStack) {
    (p.item.to_string(), label(p), p.clone())
}

fn file(world: &Path, owner: &str) -> PathBuf {
    world.join(FOLDER).join(format!("{owner}.dat"))
}

/// A storage from its file: `{version: 1, items: [{count: L, item: {id, components}}]}`.
fn read(path: &Path) -> Result<Store<ItemStack>, String> {
    let mut store = Store::default();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(store),
        Err(e) => return Err(e.to_string()),
    };
    let (root, _) = nbt::read_named(&bytes, nbt::Limits::saved()).map_err(|e| e.to_string())?;
    let Some(Tag::List(items)) = root.tag.get("items") else {
        return Err("no items list".into());
    };
    for entry in items {
        let count = match entry.get("count") {
            Some(Tag::Long(n)) => *n as u64,
            _ => continue,
        };
        let Some(stack) = entry.get("item").and_then(item_nbt::item_from_nbt) else {
            log::warn!(target: "horizon::global_storage", "Global Storage: {}: an item it can't read, skipped", path.display());
            continue;
        };
        let p = proto(&stack);
        if let Err(e) = store.add(&key(&p), count, 0, || new_entry(&p)) {
            log::warn!(target: "horizon::global_storage", "Global Storage: {}: {e:?}", path.display());
        }
    }
    store.dirty = false;
    Ok(store)
}

fn write(path: &Path, store: &Store<ItemStack>) -> Result<(), String> {
    let items = store
        .entries()
        .iter()
        .map(|e| {
            Tag::compound([
                ("count", Tag::Long(e.count as i64)),
                ("item", item_nbt::item_to_nbt(&e.item)),
            ])
        })
        .collect();
    let root = NamedTag {
        name: "".into(),
        tag: Tag::compound([("version", Tag::Int(1)), ("items", Tag::List(items))]),
    };
    let bytes = nbt::write_named(&root, nbt::Limits::saved()).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Written beside it first, then put in its place: a crash mid-write keeps the old file.
    let tmp = path.with_extension("dat.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// The storage of `owner`, read from the world on first use.
fn store<'a>(st: &'a mut State, game: &Game<'_>, owner: &str) -> &'a mut Store<ItemStack> {
    if st.world.is_none() {
        st.world = game.native_world_dir();
    }
    if !st.stores.contains_key(owner) {
        let store = match &st.world {
            Some(world) => {
                let path = file(world, owner);
                match read(&path) {
                    Ok(s) => {
                        log::info!(
                            target: "horizon::global_storage",
                            "Global Storage: {owner}: {} kinds, {} items",
                            s.types(),
                            s.total()
                        );
                        s
                    }
                    Err(e) => {
                        // Never started empty: what is put in an empty storage would be gone
                        // once the admin mends the file. Closed until then, its file untouched.
                        log::error!(
                            target: "horizon::global_storage",
                            "Global Storage: can't read {}: {e}; the storage is closed until the file is mended (or removed) and the server restarted",
                            path.display()
                        );
                        let mut s = Store::default();
                        s.locked = true;
                        s
                    }
                }
            }
            None => {
                log::warn!(target: "horizon::global_storage", "Global Storage: no world folder, the storage won't be saved");
                Store::default()
            }
        };
        st.stores.insert(owner.to_owned(), store);
    }
    st.stores.get_mut(owner).expect("inserted")
}

fn save(st: &mut State, all: bool) {
    let Some(world) = st.world.clone() else {
        return;
    };
    for (owner, store) in &mut st.stores {
        if store.locked || (!store.dirty && !all) {
            continue;
        }
        match write(&file(&world, owner), store) {
            Ok(()) => store.dirty = false,
            Err(e) => log::error!(target: "horizon::global_storage", "Global Storage: can't save {owner}: {e}"),
        }
    }
    st.last_save = Instant::now();
}

/// The world was saved, or the server stops: the storages too.
pub(in crate::game) fn flush() {
    if let Some(mut guard) = state()
        && let Some(st) = guard.as_mut()
        && st.stores.values().any(|s| s.dirty)
    {
        save(st, false);
    }
}

// ---- The players' inventories -----------------------------------------------------------

/// `f` on the player's inventory (0..9 the hotbar, 9..36 the rest), when they are here, alive
/// and nobody's state is swapped in (the menu hooks and the tick). The screen follows with
/// the tick's sync.
fn with_inventory<R>(
    game: &mut Game<'_>,
    uuid: u128,
    f: impl FnOnce(&mut [Option<ItemStack>]) -> R,
) -> Option<R> {
    if game.lan_acting.is_some() {
        return None;
    }
    let host = game.lan.as_mut()?;
    let g = host
        .hub
        .guests
        .iter_mut()
        .find(|g| g.placed && !g.dead && g.uuid == uuid)?;
    let n = g.inventory.len().min(36);
    Some(f(&mut g.inventory[..n]))
}

fn player_name(game: &Game<'_>, uuid: u128) -> Option<String> {
    let host = game.lan.as_ref()?;
    let g = host.hub.guests.iter().find(|g| g.placed && g.uuid == uuid)?;
    Some(g.name.clone())
}

fn max_stack(game: &Game<'_>, item: &str) -> u32 {
    game.res.items().get(item).map_or(64, |i| i.max_stack).max(1)
}

/// Takes `how` many of `key` out to the player: a line to tell them, if any.
fn take(
    game: &mut Game<'_>,
    store: &mut Store<ItemStack>,
    uuid: u128,
    key: &str,
    how: Take,
) -> Option<(String, u32)> {
    if store.locked {
        return Some((LOCKED.into(), text::RED));
    }
    let p = store.get(key)?.item.clone();
    let max = u64::from(max_stack(game, &p.item));
    let want = match how {
        Take::Stack | Take::Hotbar(_) => max,
        Take::One => 1,
        Take::Fill => u64::MAX,
    };
    let mut given = 0u64;
    // A number key: into that hotbar slot when it's empty.
    if let Take::Hotbar(slot) = how {
        let n = want.min(store.count(key));
        if n > 0 {
            let stack = ItemStack {
                count: n as u32,
                ..p.clone()
            };
            let placed = with_inventory(game, uuid, |inv| match inv.get_mut(slot) {
                Some(cell) if cell.is_none() => {
                    *cell = Some(stack);
                    true
                }
                _ => false,
            });
            if placed == Some(true) {
                store.take(key, n);
                return None;
            }
        }
    }
    while given < want {
        let n = (want - given).min(max).min(store.count(key));
        if n == 0 {
            break;
        }
        store.take(key, n);
        let left = game.native_give_stack(
            uuid,
            ItemStack {
                count: n as u32,
                ..p.clone()
            },
        );
        let back = left.map_or(0, |s| u64::from(s.count));
        given += n - back;
        if back > 0 {
            // Always goes back: the kind was just there.
            let _ = store.add(key, back, 0, || new_entry(&p));
            break;
        }
    }
    (given == 0).then(|| ("В інвентарі немає місця.".into(), text::RED))
}

#[derive(Default)]
struct Moved {
    items: u64,
    stacks: u32,
    banned: u32,
    full: bool,
    too_big: bool,
    no_room: bool,
    locked: bool,
}

/// Puts the stacks of these inventory slots in, those `only` allows.
fn deposit(
    game: &mut Game<'_>,
    store: &mut Store<ItemStack>,
    config: &Config,
    uuid: u128,
    slots: &[usize],
    only: impl Fn(&str) -> bool,
) -> Moved {
    let mut moved = Moved::default();
    with_inventory(game, uuid, |inv| {
        for &s in slots {
            let Some(cell) = inv.get_mut(s) else {
                continue;
            };
            let Some(stack) = cell.as_ref() else {
                continue;
            };
            if !config.allowed(&stack.item) {
                moved.banned += 1;
                continue;
            }
            let p = proto(stack);
            let k = key(&p);
            if !only(&k) {
                continue;
            }
            let n = u64::from(stack.count);
            match store.fits(&k, config.max_item_bytes(), config.max_storage_bytes()) {
                Err(Refused::TooBig) => {
                    moved.too_big = true;
                    continue;
                }
                Err(Refused::Locked) => {
                    moved.locked = true;
                    break;
                }
                Err(_) => {
                    moved.no_room = true;
                    continue;
                }
                Ok(()) => {}
            }
            match store.add(&k, n, config.max_types, || new_entry(&p)) {
                Ok(()) => {
                    *cell = None;
                    moved.items += n;
                    moved.stacks += 1;
                }
                Err(Refused::Locked) => {
                    moved.locked = true;
                    break;
                }
                Err(_) => moved.full = true,
            }
        }
    });
    moved
}

fn tell_moved(game: &mut Game<'_>, uuid: u128, m: &Moved, quiet: bool) {
    if m.items > 0 && !quiet {
        game.native_tell(
            uuid,
            Text::literal(format!(
                "Покладено в сховище: {} шт. ({} ст.)",
                global_storage_core::group(m.items),
                m.stacks
            ))
            .with_color(text::GREEN),
        );
    }
    if m.banned > 0 {
        game.native_tell(
            uuid,
            Text::literal("Цей предмет не можна класти в сховище.").with_color(text::RED),
        );
    }
    if m.too_big {
        game.native_tell(
            uuid,
            Text::literal("Цей предмет завеликий для сховища (наприклад, книга, повна тексту, або шалкер із такими книгами).")
                .with_color(text::RED),
        );
    }
    if m.no_room {
        game.native_tell(
            uuid,
            Text::literal("Сховище заповнене за розміром: нових видів предметів воно більше не вміщує.")
                .with_color(text::RED),
        );
    }
    if m.locked {
        game.native_tell(uuid, Text::literal(LOCKED).with_color(text::RED));
    }
    if m.full {
        game.native_tell(
            uuid,
            Text::literal("Сховище заповнене: більше нових видів предметів воно не вміщує.")
                .with_color(text::RED),
        );
    }
}

// ---- The menu ---------------------------------------------------------------------------

fn line(text: &str, color: &str) -> Tag {
    component_nbt(&serde_json::json!({ "text": text, "italic": false, "color": color }))
}

fn icon(item: &str, title: &str, color: &str, lore: &[gui::Line]) -> ItemStack {
    let mut s = ItemStack::new(item, 1);
    s.data_mut().custom_name = Some(line(title, color));
    let lore = lore.iter().map(|(t, c)| line(t, c)).collect();
    s.data_mut().set_other("minecraft:lore", Some(Tag::List(lore)));
    s
}

/// An entry as the menu shows it: the item itself, its own lore, then how many there are.
fn shown(item: &ItemStack, count: u64, max: u32) -> ItemStack {
    let mut s = ItemStack {
        count: count.clamp(1, 99) as u32,
        ..item.clone()
    };
    let mut lore = match s.data().and_then(|d| d.other("minecraft:lore")) {
        Some(Tag::List(l)) => l.clone(),
        _ => Vec::new(),
    };
    lore.extend(gui::entry_lore(count, max).iter().map(|(t, c)| line(t, c)));
    s.data_mut().set_other("minecraft:lore", Some(Tag::List(lore)));
    s
}

/// Lays the view out again from its storage: the items to send.
fn render(
    game: &Game<'_>,
    view: &mut View,
    store: &Store<ItemStack>,
    names: &Names,
    creative: &Creative,
    config: &Config,
) -> Vec<Option<ItemStack>> {
    let query = Query::parse(&view.query);
    let look = Look {
        query: &query,
        tab: Tab::All,
        sort: view.sort,
    };
    let shown_ids = view::select(store, names, creative, &look);
    let pages = view::pages(shown_ids.len(), gui::PER_PAGE);
    view.page = view.page.min(pages - 1);
    view.slots = gui::layout(&shown_ids, view.page);
    let status = gui::Status {
        page: view.page,
        pages,
        types: store.types(),
        total: store.total(),
        shown: shown_ids.len(),
        query: &view.query,
        sort: view.sort,
        deposit_hotbar: config.deposit_hotbar,
        shared: config.mode == Mode::Global,
    };
    let entries = store.entries();
    view.keys = view
        .slots
        .iter()
        .map(|s| match s {
            Slot::Entry(i) => Some(entries[*i].key.clone()),
            _ => None,
        })
        .collect();
    view.slots
        .iter()
        .map(|s| match *s {
            Slot::Entry(i) => {
                let e = &entries[i];
                Some(shown(&e.item, e.count, max_stack(game, &e.item.item)))
            }
            Slot::Button(b) => {
                let (item, title, color, lore) = gui::button(b, &status);
                Some(icon(item, &title, color, &lore))
            }
            Slot::Filler => Some(icon("minecraft:gray_stained_glass_pane", " ", gui::GRAY, &[])),
            Slot::Empty => None,
        })
        .collect()
}

/// Tells a terminal what changed in its storage since it was last told (everything, the
/// first time): the records go to the player's outbox, sent from the tick.
fn sync_terminal(
    uuid: u128,
    view: &mut View,
    store: &Store<ItemStack>,
    ids: &mut Ids,
    names: &Names,
    outbox: &mut HashMap<u128, VecDeque<Vec<u8>>>,
    open: Option<&str>,
) {
    let mut records = Vec::new();
    if let Some(title) = open {
        view.sent.clear();
        records.push(term::record(&["open", &term::clean(title)]));
    }
    let now: Vec<(u32, u64, usize)> = store
        .entries()
        .iter()
        .enumerate()
        .map(|(i, e)| (ids.of(&e.key), e.count, i))
        .collect();
    let index: HashMap<u32, usize> = now.iter().map(|&(id, _, i)| (id, i)).collect();
    let (add, set, del) = term::diff(&mut view.sent, now.iter().map(|&(id, c, _)| (id, c)));
    for id in add {
        let e = &store.entries()[index[&id]];
        let custom = e.item.data().and_then(|d| d.custom_name_plain());
        let name = custom.unwrap_or_else(|| names.display(&e.id));
        let hay = format!("{} {}", names.haystack(&e.id), e.label);
        let snbt = horizon_world::snbt_text::to_snbt(&item_nbt::item_to_nbt(&e.item));
        records.push(term::record(&[
            "add",
            &id.to_string(),
            &e.count.to_string(),
            &term::clean(&name),
            &term::clean(&hay),
            &term::clean(&snbt),
        ]));
    }
    for (id, count) in set {
        records.push(term::record(&["set", &id.to_string(), &count.to_string()]));
    }
    for id in del {
        records.push(term::record(&["del", &id.to_string()]));
    }
    if records.is_empty() {
        return;
    }
    outbox
        .entry(uuid)
        .or_default()
        .extend(term::pack(&records, term::MAX_MESSAGE));
}

/// Every open menu of `owner`'s storage shows it as it is now: a terminal hears what changed,
/// a chest is sent its items again.
fn refresh(game: &mut Game<'_>, st: &mut State, owner: &str) {
    let State {
        views,
        stores,
        names,
        creative,
        config,
        ids,
        outbox,
        ..
    } = st;
    let Some(store) = stores.get(owner) else {
        return;
    };
    let ids = ids.entry(owner.to_owned()).or_default();
    let mut sends = Vec::new();
    for (uuid, view) in views.iter_mut().filter(|(_, v)| v.owner == owner) {
        if view.terminal {
            sync_terminal(*uuid, view, store, ids, names, outbox, None);
        } else {
            sends.push((*uuid, render(game, view, store, names, creative, config)));
        }
    }
    for (uuid, items) in sends {
        game.native_set_menu_items(uuid, items);
    }
}

fn open(game: &mut Game<'_>, st: &mut State, uuid: u128, query: String) {
    let owner = owner(&st.config, uuid);
    store(st, game, &owner);
    let sort = st.sorts.get(&uuid).copied().unwrap_or_default();
    // A client that said hello lately draws the terminal over the chest.
    let terminal = st
        .terminals
        .get(&uuid)
        .is_some_and(|t| t.elapsed() < TERMINAL_FRESH);
    let mut view = View {
        owner: owner.clone(),
        window: None,
        query,
        sort,
        page: 0,
        slots: Vec::new(),
        keys: Vec::new(),
        last_deposit: None,
        terminal,
        sent: HashMap::new(),
    };
    let items = if terminal {
        // The chest's own slots stay empty: the terminal draws over them.
        vec![None; gui::SIZE]
    } else {
        render(
            game,
            &mut view,
            &st.stores[&owner],
            &st.names,
            &st.creative,
            &st.config,
        )
    };
    let title = gui::title(&st.config.title, &view.query);
    view.window = game.native_open_menu(uuid, gui::ROWS, Text::literal(title.clone()), items);
    if view.window.is_none() {
        return;
    }
    if terminal {
        let State {
            stores,
            ids,
            names,
            outbox,
            ..
        } = &mut *st;
        let ids = ids.entry(owner.clone()).or_default();
        // Anything still queued for an older terminal is out of date.
        outbox.remove(&uuid);
        sync_terminal(
            uuid,
            &mut view,
            &stores[&owner],
            ids,
            names,
            outbox,
            Some(&title),
        );
    }
    st.views.insert(uuid, view);
}

/// SNBT text in double quotes.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The search box: a dialog whose button runs `/gs =<text>`.
fn search_dialog(game: &mut Game<'_>, uuid: u128, current: &str) {
    let Some(name) = player_name(game, uuid) else {
        return;
    };
    let dialog = format!(
        "{{type:\"minecraft:confirmation\",title:{title},\
         body:{{type:\"minecraft:plain_message\",contents:{body}}},\
         inputs:[{{type:\"minecraft:text\",key:\"q\",label:{label},width:256,max_length:64,initial:{initial}}}],\
         yes:{{label:{yes},action:{{type:\"minecraft:dynamic/run_command\",template:\"gs {MARK}$(q)\"}}}},\
         no:{{label:{no},action:{{type:\"minecraft:run_command\",command:\"gs\"}}}}}}",
        title = quoted("Пошук у сховищі"),
        body = quoted("Назва українською чи англійською, id (diamond_sword) або @мод (@create)."),
        label = quoted("Що шукаємо?"),
        initial = quoted(current),
        yes = quoted("Шукати"),
        no = quoted("Показати все"),
    );
    game.native_close_menu(uuid);
    if !game.native_run_command(&format!("dialog show {name} {dialog}")) {
        game.native_tell(
            uuid,
            Text::literal("Пошук: напишіть у чаті /gs <назва предмета>").with_color(text::YELLOW),
        );
    }
}

/// Each tick: open the menus asked for, save what changed now and then.
pub(in crate::game) fn tick(game: &mut Game<'_>) {
    let Some(mut guard) = state() else {
        return;
    };
    let Some(st) = guard.as_mut() else {
        return;
    };
    if !st.pending.is_empty() {
        for (uuid, query) in std::mem::take(&mut st.pending) {
            open(game, st, uuid, query);
        }
    }
    if !st.outbox.is_empty() {
        for (uuid, queue) in st.outbox.iter_mut() {
            for _ in 0..SENDS_PER_TICK {
                let Some(msg) = queue.pop_front() else {
                    break;
                };
                if game.native_net_send_to_player(*uuid, term::TO_CLIENT, &msg).is_err() {
                    // Gone, or not on a client that hears mod messages.
                    queue.clear();
                }
            }
        }
        st.outbox.retain(|_, q| !q.is_empty());
    }
    // A player who moved items and left was saved on leaving: the storage follows at once.
    let left = st.touched.iter().any(|u| !placed(game, *u));
    let dirty = st.stores.values().any(|s| s.dirty);
    if (dirty && left)
        || (st.last_save.elapsed().as_secs() >= st.config.save_every
            && (dirty || !st.touched.is_empty()))
    {
        save(st, false);
        save_players(game, std::mem::take(&mut st.touched));
    }
}

fn placed(game: &Game<'_>, uuid: u128) -> bool {
    game.lan
        .as_ref()
        .is_some_and(|h| h.hub.guests.iter().any(|g| g.placed && g.uuid == uuid))
}

/// The files of these players, written now (queued behind the autosaves already running):
/// the same moment as the storage they moved items to or from.
fn save_players(game: &mut Game<'_>, players: HashSet<u128>) {
    if players.is_empty() || game.options.dedicated.is_none() {
        return;
    }
    let Some(mut host) = game.lan.take() else {
        return;
    };
    for g in host
        .hub
        .guests
        .iter_mut()
        .filter(|g| g.placed && players.contains(&g.uuid))
    {
        game.guest_attachments_into(g);
        game.player_autosaves.enqueue(g, &game.world_root);
    }
    game.lan = Some(host);
}

/// A click in a menu: true when it was ours.
pub(in crate::game) fn menu_clicked(
    game: &mut Game<'_>,
    uuid: u128,
    window: i32,
    slot: i32,
    button: i32,
    mode: i32,
) -> bool {
    let Some(mut guard) = state() else {
        return false;
    };
    let Some(st) = guard.as_mut() else {
        return false;
    };
    let Some(view) = st.views.get(&uuid).filter(|v| v.window == Some(window)) else {
        return false;
    };
    let owner = view.owner.clone();
    let config = st.config.clone();
    let action = gui::action(slot, button, mode);
    if !matches!(action, Action::None) {
        st.touched.insert(uuid);
    }
    // A terminal draws the storage itself and sends its own actions: of the chest's clicks
    // only those on the inventory below count.
    if view.terminal && !matches!(action, Action::Deposit(_) | Action::DepositAll(_)) {
        return true;
    }
    match action {
        Action::Take(s, how) => {
            let Some(k) = view.keys.get(s).cloned().flatten() else {
                return true;
            };
            let store = store(st, game, &owner);
            if let Some((msg, color)) = take(game, store, uuid, &k, how) {
                game.native_tell(uuid, Text::literal(msg).with_color(color));
            }
        }
        Action::Deposit(inv) => {
            let like = with_inventory(game, uuid, |i| i.get(inv).cloned().flatten()).flatten();
            let store = store(st, game, &owner);
            let m = deposit(game, store, &config, uuid, &[inv], |_| true);
            tell_moved(game, uuid, &m, true);
            if let (Some(like), true) = (like, m.items > 0)
                && let Some(v) = st.views.get_mut(&uuid)
            {
                v.last_deposit = Some((key(&proto(&like)), Instant::now()));
            }
        }
        Action::DepositAll(inv) => {
            let like = with_inventory(game, uuid, |i| i.get(inv).cloned().flatten())
                .flatten()
                .map(|s| key(&proto(&s)))
                .or_else(|| {
                    let (k, at) = view.last_deposit.clone()?;
                    (at.elapsed().as_millis() < 1500).then_some(k)
                });
            if let Some(k) = like {
                let store = store(st, game, &owner);
                let m = deposit(game, store, &config, uuid, &(0..36).collect::<Vec<_>>(), |x| {
                    x == k
                });
                tell_moved(game, uuid, &m, true);
            }
        }
        Action::Press(s, alt) => {
            let Some(Slot::Button(b)) = view.slots.get(s).copied() else {
                return true;
            };
            let query = view.query.clone();
            match b {
                Button::Prev | Button::Next => {
                    let v = st.views.get_mut(&uuid).expect("found");
                    v.page = match (b, alt) {
                        (Button::Prev, false) => v.page.saturating_sub(1),
                        (Button::Prev, true) => 0,
                        (_, false) => v.page + 1,
                        (_, true) => usize::MAX,
                    };
                }
                Button::Sort => {
                    let v = st.views.get_mut(&uuid).expect("found");
                    v.sort = if alt { v.sort.prev() } else { v.sort.next() };
                    v.page = 0;
                    st.sorts.insert(uuid, v.sort);
                }
                Button::Search if alt => {
                    if !query.is_empty() {
                        st.pending.push((uuid, String::new()));
                    }
                }
                Button::Search => search_dialog(game, uuid, &query),
                Button::Info => {}
                Button::Deposit => deposit_button(game, st, uuid, &owner, &config, alt),
            }
        }
        Action::None => {}
    }
    refresh(game, st, &owner);
    true
}

/// A menu closed: true when it was ours.
pub(in crate::game) fn menu_closed(uuid: u128, window: i32) -> bool {
    let Some(mut guard) = state() else {
        return false;
    };
    let Some(st) = guard.as_mut() else {
        return false;
    };
    let ours = st.views.get(&uuid).is_some_and(|v| v.window == Some(window));
    if ours && let Some(view) = st.views.remove(&uuid)
        && view.terminal
    {
        st.outbox
            .entry(uuid)
            .or_default()
            .extend(term::pack(&[term::record(&["close"])], term::MAX_MESSAGE));
    }
    ours
}
