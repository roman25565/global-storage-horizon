//! The creative inventory's tabs, so the storage is browsed as players know it.
//!
//! `data/creative.tsv` is made from Horizon's export of the vanilla creative tabs
//! (`creative-inventory.json`, 26.3): each item id, the first tab it is in, and its place in
//! the creative order. An item no tab lists (another mod's) goes to [`Tab::Other`].

use std::collections::HashMap;

const TABLE: &str = include_str!("../data/creative.tsv");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Tab {
    /// Every item.
    #[default]
    All,
    Building,
    Colored,
    Natural,
    Functional,
    Redstone,
    Tools,
    Combat,
    Food,
    Ingredients,
    SpawnEggs,
    /// Operator blocks and items of mods.
    Other,
}

impl Tab {
    pub const ALL: [Tab; 12] = [
        Tab::All,
        Tab::Building,
        Tab::Colored,
        Tab::Natural,
        Tab::Functional,
        Tab::Redstone,
        Tab::Tools,
        Tab::Combat,
        Tab::Food,
        Tab::Ingredients,
        Tab::SpawnEggs,
        Tab::Other,
    ];

    /// The tab of the table's column (the export's order of category tabs).
    fn from_column(n: u32) -> Tab {
        match n {
            0 => Tab::Building,
            1 => Tab::Colored,
            2 => Tab::Natural,
            3 => Tab::Functional,
            4 => Tab::Redstone,
            5 => Tab::Tools,
            6 => Tab::Combat,
            7 => Tab::Food,
            8 => Tab::Ingredients,
            9 => Tab::SpawnEggs,
            _ => Tab::Other,
        }
    }

    /// The game's own Ukrainian names for the tabs.
    pub fn title(self) -> &'static str {
        match self {
            Tab::All => "Усі предмети",
            Tab::Building => "Будівельні блоки",
            Tab::Colored => "Кольорові блоки",
            Tab::Natural => "Природні блоки",
            Tab::Functional => "Функціональні блоки",
            Tab::Redstone => "Блоки редстоуну",
            Tab::Tools => "Інструменти й прилади",
            Tab::Combat => "Бойове приладдя",
            Tab::Food => "Їжа та напої",
            Tab::Ingredients => "Інгредієнти",
            Tab::SpawnEggs => "Яйця виклику",
            Tab::Other => "Інше (моди, оператор)",
        }
    }

    /// The creative tab's own icon.
    pub fn icon(self) -> &'static str {
        match self {
            Tab::All => "minecraft:compass",
            Tab::Building => "minecraft:bricks",
            Tab::Colored => "minecraft:cyan_wool",
            Tab::Natural => "minecraft:grass_block",
            Tab::Functional => "minecraft:oak_sign",
            Tab::Redstone => "minecraft:redstone",
            Tab::Tools => "minecraft:diamond_pickaxe",
            Tab::Combat => "minecraft:netherite_sword",
            Tab::Food => "minecraft:golden_apple",
            Tab::Ingredients => "minecraft:iron_ingot",
            Tab::SpawnEggs => "minecraft:creeper_spawn_egg",
            Tab::Other => "minecraft:command_block",
        }
    }
}

pub struct Creative {
    map: HashMap<&'static str, (Tab, u32)>,
}

impl Default for Creative {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Creative {
    pub fn builtin() -> Self {
        let map = TABLE
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                let id = f.next()?;
                let tab = Tab::from_column(f.next()?.parse().ok()?);
                let order = f.next()?.parse().ok()?;
                Some((id, (tab, order)))
            })
            .collect();
        Self { map }
    }

    /// The item's tab, never [`Tab::All`].
    pub fn tab(&self, id: &str) -> Tab {
        self.map.get(id).map_or(Tab::Other, |(t, _)| *t)
    }

    /// The item's place in the creative order; items no tab lists come last.
    pub fn order(&self, id: &str) -> u32 {
        self.map.get(id).map_or(u32::MAX, |(_, o)| *o)
    }

    /// Whether `id` shows in `tab`.
    pub fn shows(&self, tab: Tab, id: &str) -> bool {
        tab == Tab::All || self.tab(id) == tab
    }
}
