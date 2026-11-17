use crate::{
    config::{self, Config, Mode},
    creative::{Creative, Tab},
    gui::{self, Action, Button, Slot, Take},
    names::Names,
    store::{Refused, Store},
    view::{self, Look, Query, Sort},
};

fn select(s: &Store<String>, q: &str, tab: Tab, sort: Sort) -> Vec<String> {
    let query = Query::parse(q);
    view::select(s, &Names::builtin(), &Creative::builtin(), &Look { query: &query, tab, sort })
        .into_iter()
        .map(|i| s.entries()[i].id.clone())
        .collect()
}

fn put(s: &mut Store<String>, id: &str, n: u64) {
    s.add(id, n, 0, || (id.to_owned(), String::new(), id.to_owned()))
        .unwrap();
}

#[test]
fn counts_have_no_stack_limit() {
    let mut s = Store::default();
    put(&mut s, "minecraft:cobblestone", 64);
    put(&mut s, "minecraft:cobblestone", 1_000_000);
    put(&mut s, "minecraft:diamond_sword", 3);
    assert_eq!(s.count("minecraft:cobblestone"), 1_000_064);
    assert_eq!(s.types(), 2);
    assert_eq!(s.total(), 1_000_067);
    // u64 is the limit; going past it is refused, nothing changes.
    put(&mut s, "minecraft:dirt", u64::MAX);
    assert_eq!(
        s.add("minecraft:dirt", 1, 0, || unreachable!()),
        Err(Refused::Overflow)
    );
    assert_eq!(s.count("minecraft:dirt"), u64::MAX);
}

#[test]
fn taking_never_makes_items() {
    let mut s = Store::default();
    put(&mut s, "a", 10);
    put(&mut s, "b", 5);
    put(&mut s, "c", 7);
    let v = s.version;
    assert_eq!(s.take("a", 4), 4);
    assert_eq!(s.take("a", 100), 6);
    assert_eq!(s.take("a", 1), 0);
    assert_eq!(s.take("nothing", 1), 0);
    assert!(s.version > v);
    // The emptied entry is gone and the others are still found by key.
    assert_eq!(s.types(), 2);
    assert_eq!(s.count("b"), 5);
    assert_eq!(s.count("c"), 7);
    assert_eq!(s.get("c").unwrap().key, "c");
    assert_eq!(s.take("c", 7), 7);
    assert_eq!(s.take("b", 5), 5);
    assert_eq!(s.types(), 0);
    assert!(s.dirty);
}

#[test]
fn max_types_refuses_new_kinds_only() {
    let mut s = Store::default();
    s.add("a", 1, 2, || ("a".into(), String::new(), String::new()))
        .unwrap();
    s.add("b", 1, 2, || ("b".into(), String::new(), String::new()))
        .unwrap();
    assert_eq!(
        s.add("c", 1, 2, || unreachable!()),
        Err(Refused::Full)
    );
    // More of a kind already in is fine.
    s.add("a", 99, 2, || unreachable!()).unwrap();
    assert_eq!(s.count("a"), 100);
}

#[test]
fn search_finds_ukrainian_english_ids_and_mods() {
    let names = Names::builtin();
    let mut s = Store::default();
    for id in [
        "minecraft:diamond",
        "minecraft:diamond_sword",
        "minecraft:oak_log",
        "create:brass_ingot",
    ] {
        put(&mut s, id, 1);
    }
    s.add("named", 1, 0, || {
        ("minecraft:stick".into(), "чарівна паличка".into(), String::new())
    })
    .unwrap();
    let _ = &names;
    let find = |q: &str| -> Vec<String> {
        let mut ids = select(&s, q, Tab::All, Sort::Name);
        ids.sort();
        ids
    };
    assert_eq!(find("діамант").len(), 2, "{:?}", find("діамант"));
    assert_eq!(find("Діамантовий МЕЧ"), ["minecraft:diamond_sword"]);
    assert_eq!(find("diamond sword"), ["minecraft:diamond_sword"]);
    assert_eq!(find("oak_log"), ["minecraft:oak_log"]);
    assert_eq!(find("brass"), ["create:brass_ingot"]);
    assert_eq!(find("@create"), ["create:brass_ingot"]);
    assert_eq!(find("@cre ingot"), ["create:brass_ingot"]);
    assert_eq!(find("@minecraft").len(), 4);
    assert_eq!(find("чарівна"), ["minecraft:stick"]);
    assert_eq!(find("немає такого"), Vec::<String>::new());
    assert_eq!(find("").len(), 5);
    assert_eq!(find("   ").len(), 5);
}

#[test]
fn sorting() {
    let names = Names::builtin();
    let mut s = Store::default();
    put(&mut s, "minecraft:stone", 5);
    put(&mut s, "minecraft:apple", 50);
    put(&mut s, "create:zinc_ingot", 7);
    let _ = &names;
    let order = |sort| -> Vec<String> { select(&s, "", Tab::All, sort) };
    assert_eq!(
        order(Sort::Count),
        ["minecraft:apple", "create:zinc_ingot", "minecraft:stone"]
    );
    // Ukrainian names: Zinc Ingot (no translation) < Камінь < Яблуко.
    assert_eq!(
        order(Sort::Name),
        ["create:zinc_ingot", "minecraft:stone", "minecraft:apple"]
    );
    assert_eq!(
        order(Sort::Id),
        ["create:zinc_ingot", "minecraft:apple", "minecraft:stone"]
    );
    // As the creative inventory: building blocks (stone) before food (apple), mods last.
    assert_eq!(
        order(Sort::Creative),
        ["minecraft:stone", "minecraft:apple", "create:zinc_ingot"]
    );
    assert_eq!(Sort::default(), Sort::Creative);
    assert_eq!(Sort::Count.next().next().next().next(), Sort::Count);
    assert_eq!(Sort::Name.prev(), Sort::Count);
    assert_eq!(Sort::Creative.prev(), Sort::Id);
}

#[test]
fn names() {
    let n = Names::builtin();
    assert_eq!(n.display("minecraft:diamond_sword"), "Діамантовий меч");
    assert_eq!(n.display("minecraft:stone"), "Камінь");
    assert_eq!(n.display("create:brass_ingot"), "Brass Ingot");
    assert!(n.haystack("minecraft:oak_log").contains("oak log"));
}

#[test]
fn pages_and_layout() {
    assert_eq!(view::pages(0, 45), 1);
    assert_eq!(view::pages(45, 45), 1);
    assert_eq!(view::pages(46, 45), 2);
    let shown: Vec<usize> = (0..100).collect();
    let first = gui::layout(&shown, 0);
    assert_eq!(first.len(), gui::SIZE);
    assert_eq!(first[0], Slot::Entry(0));
    assert_eq!(first[44], Slot::Entry(44));
    assert_eq!(first[gui::PREV], Slot::Filler);
    assert_eq!(first[gui::NEXT], Slot::Button(Button::Next));
    let last = gui::layout(&shown, 2);
    assert_eq!(last[0], Slot::Entry(90));
    assert_eq!(last[9], Slot::Entry(99));
    assert_eq!(last[10], Slot::Empty);
    assert_eq!(last[gui::PREV], Slot::Button(Button::Prev));
    assert_eq!(last[gui::NEXT], Slot::Filler);
    assert_eq!(gui::layout(&shown, 99), last);
    let empty = gui::layout(&[], 0);
    assert_eq!(empty[0], Slot::Empty);
    assert_eq!(empty[gui::SEARCH], Slot::Button(Button::Search));
    assert_eq!(empty[gui::SORT], Slot::Button(Button::Sort));
    assert_eq!(empty[gui::DEPOSIT], Slot::Button(Button::Deposit));
    assert_eq!(empty[gui::INFO], Slot::Button(Button::Info));
}

#[test]
fn terminal_protocol() {
    use crate::term::{self, Act, Ids};
    let mut ids = Ids::default();
    let a = ids.of("a");
    let b = ids.of("b");
    assert_ne!(a, b);
    assert_eq!(ids.of("a"), a);
    assert_eq!(ids.key(b), Some("b"));
    let mut sent = std::collections::HashMap::new();
    let (add, set, del) = term::diff(&mut sent, [(1, 10), (2, 5)]);
    assert_eq!((add.len(), set.len(), del.len()), (2, 0, 0));
    let (add, set, del) = term::diff(&mut sent, [(1, 10), (2, 7), (3, 1)]);
    assert_eq!((add, set, del), (vec![3], vec![(2, 7)], vec![]));
    let (add, set, del) = term::diff(&mut sent, [(3, 1)]);
    assert_eq!((add, set, del), (vec![], vec![], vec![1, 2]));
    assert_eq!(sent.len(), 1);
    let records: Vec<String> = (0..1000).map(|i| term::record(&["set", &i.to_string(), "64"])).collect();
    let msgs = term::pack(&records, 1000);
    assert!(msgs.iter().all(|m| m.len() <= 1000));
    let back: Vec<String> = msgs
        .iter()
        .flat_map(|m| {
            String::from_utf8(m.clone())
                .unwrap()
                .split(term::RECORD)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(back, records);
    assert_eq!(term::clean("a\u{1f}b\u{1e}c"), "a b c");
    assert_eq!(term::parse_act("take\u{1f}12\u{1f}fill"), Some(Act::Take(12, Take::Fill)));
    assert_eq!(term::parse_act("take\u{1f}12\u{1f}one"), Some(Act::Take(12, Take::One)));
    assert_eq!(term::parse_act("deposit\u{1f}known"), Some(Act::Deposit(true)));
    assert_eq!(term::parse_act("take\u{1f}x\u{1f}one"), None);
    assert_eq!(term::parse_act("nonsense"), None);
    assert_eq!(term::hello_version("hello:4"), Some(4));
    assert_eq!(term::hello_version("hello"), None);
}

#[test]
fn creative_tabs() {
    let c = Creative::builtin();
    assert_eq!(c.tab("minecraft:stone"), Tab::Building);
    assert_eq!(c.tab("minecraft:red_wool"), Tab::Colored);
    assert_eq!(c.tab("minecraft:oak_sapling"), Tab::Natural);
    assert_eq!(c.tab("minecraft:crafting_table"), Tab::Functional);
    assert_eq!(c.tab("minecraft:repeater"), Tab::Redstone);
    assert_eq!(c.tab("minecraft:diamond_pickaxe"), Tab::Tools);
    assert_eq!(c.tab("minecraft:diamond_sword"), Tab::Combat);
    assert_eq!(c.tab("minecraft:bread"), Tab::Food);
    assert_eq!(c.tab("minecraft:iron_ingot"), Tab::Ingredients);
    assert_eq!(c.tab("minecraft:zombie_spawn_egg"), Tab::SpawnEggs);
    assert_eq!(c.tab("minecraft:command_block"), Tab::Other);
    assert_eq!(c.tab("create:brass_ingot"), Tab::Other);
    assert!(c.order("minecraft:oak_log") < c.order("minecraft:oak_planks"));
    assert_eq!(c.order("create:brass_ingot"), u32::MAX);

    let mut s = Store::default();
    for id in ["minecraft:bread", "minecraft:stone", "minecraft:diamond_sword", "create:cog"] {
        put(&mut s, id, 1);
    }
    assert_eq!(select(&s, "", Tab::Food, Sort::Count), ["minecraft:bread"]);
    assert_eq!(select(&s, "", Tab::Other, Sort::Count), ["create:cog"]);
    assert_eq!(select(&s, "", Tab::Redstone, Sort::Count), Vec::<String>::new());
    assert_eq!(select(&s, "", Tab::All, Sort::Count).len(), 4);
    // The search works inside the tab.
    assert_eq!(select(&s, "меч", Tab::Combat, Sort::Count), ["minecraft:diamond_sword"]);
    assert_eq!(select(&s, "меч", Tab::Food, Sort::Count), Vec::<String>::new());
}

#[test]
fn clicks() {
    assert_eq!(gui::action(3, 0, 0), Action::Take(3, Take::Stack));
    assert_eq!(gui::action(12, 1, 0), Action::Take(12, Take::One));
    assert_eq!(gui::action(12, 0, 1), Action::Take(12, Take::Fill));
    assert_eq!(gui::action(12, 4, 2), Action::Take(12, Take::Hotbar(4)));
    // The off-hand key, a drag, a double click on the storage: nothing.
    assert_eq!(gui::action(12, 40, 2), Action::None);
    assert_eq!(gui::action(12, 0, 5), Action::None);
    assert_eq!(gui::action(12, 0, 6), Action::None);
    assert_eq!(gui::action(gui::SEARCH as i32, 0, 0), Action::Press(gui::SEARCH, false));
    assert_eq!(gui::action(gui::SEARCH as i32, 1, 0), Action::Press(gui::SEARCH, true));
    assert_eq!(gui::action(gui::SORT as i32, 0, 1), Action::Press(gui::SORT, true));
    // Below the menu: the inventory's 27, then the hotbar.
    assert_eq!(gui::action(54, 0, 0), Action::Deposit(9));
    assert_eq!(gui::action(80, 1, 1), Action::Deposit(35));
    assert_eq!(gui::action(81, 0, 0), Action::Deposit(0));
    assert_eq!(gui::action(89, 0, 6), Action::DepositAll(8));
    assert_eq!(gui::action(90, 0, 0), Action::None);
    assert_eq!(gui::action(-999, 0, 0), Action::None);
}

#[test]
fn text() {
    assert_eq!(crate::group(0), "0");
    assert_eq!(crate::group(999), "999");
    assert_eq!(crate::group(1000), "1 000");
    assert_eq!(crate::group(1234567), "1 234 567");
    assert_eq!(crate::stacks(130, 64), "2 ст. + 2");
    assert_eq!(crate::stacks(128, 64), "2 ст.");
    assert_eq!(crate::stacks(5, 1), "5");
    assert_eq!(crate::short(12_345), "12.3 тис.");
    assert_eq!(gui::title("Сховище", ""), "Сховище");
    assert_eq!(gui::title("Сховище", "алмаз"), "Сховище: алмаз");
    let lore = gui::entry_lore(130, 64);
    assert_eq!(lore[0].0, "У сховищі: 130");
    assert_eq!(lore[1].0, "(2 ст. + 2)");
}

#[test]
fn config_defaults_and_errors() {
    let c = Config::default();
    assert_eq!(c.mode, Mode::Global);
    assert_eq!(c.permission, 0);
    assert_eq!(c.max_types, 0);
    assert!(!c.deposit_hotbar);
    assert!(c.blacklist.is_empty());
    let c = config::parse(
        "mode = \"player\"\nblacklist = [\"minecraft:bedrock\", \"badmod:*\"]\npermission = 2",
    )
    .unwrap();
    assert_eq!(c.mode, Mode::PerPlayer);
    assert_eq!(c.permission, 2);
    assert!(!c.allowed("minecraft:bedrock"));
    assert!(!c.allowed("badmod:anything"));
    assert!(c.allowed("minecraft:stone"));
    assert!(config::parse("mode = \"both\"").is_err());
    assert!(config::parse("permision = 2").is_err());
    assert!(config::parse("save_every = 0").is_err());
}

#[test]
fn size_limits_refuse_only_new_kinds() {
    let mut s = Store::default();
    put(&mut s, "small", 1);
    assert_eq!(s.key_bytes(), 5);
    // One item bigger than the limit: refused.
    assert_eq!(s.fits(&"x".repeat(11), 10, 0), Err(Refused::TooBig));
    // The storage would pass its size: refused; a kind already there still fits.
    assert_eq!(s.fits("abcdef", 0, 10), Err(Refused::NoRoom));
    assert_eq!(s.fits("small", 1, 1), Ok(()));
    assert_eq!(s.fits("abcde", 0, 10), Ok(()));
    // No limits.
    assert_eq!(s.fits(&"y".repeat(1000), 0, 0), Ok(()));
    // Emptied kinds give their bytes back.
    put(&mut s, "abcde", 2);
    assert_eq!(s.key_bytes(), 10);
    assert_eq!(s.take("small", 1), 1);
    assert_eq!(s.key_bytes(), 5);
    assert_eq!(s.take("abcde", 1), 1);
    assert_eq!(s.key_bytes(), 5);
}

#[test]
fn a_locked_storage_moves_nothing() {
    let mut s = Store::default();
    put(&mut s, "a", 3);
    s.locked = true;
    assert_eq!(s.fits("b", 0, 0), Err(Refused::Locked));
    assert_eq!(s.add("a", 1, 0, || unreachable!()), Err(Refused::Locked));
    assert_eq!(s.take("a", 1), 0);
    assert_eq!(s.count("a"), 3);
}

#[test]
fn size_settings_parse() {
    let c = Config::default();
    assert_eq!(c.max_item_bytes(), 128 * 1024);
    assert_eq!(c.max_storage_bytes(), 16 * 1024 * 1024);
    let c = config::parse("max_item_kb = 0\nmax_storage_mb = 2").unwrap();
    assert_eq!((c.max_item_bytes(), c.max_storage_bytes()), (0, 2 * 1024 * 1024));
    assert!(config::parse("max_storage_mb = 5000").is_err());
}
