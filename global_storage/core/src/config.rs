//! `config/global_storage/global-storage.toml`.

/// The file as written on the first start.
pub const DEFAULT: &str = include_str!("../data/global-storage.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One storage for everyone.
    Global,
    /// A storage per player.
    PerPlayer,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub mode: Mode,
    pub title: String,
    pub permission: u8,
    pub max_types: usize,
    pub deposit_hotbar: bool,
    pub blacklist: Vec<String>,
    pub save_every: u64,
    /// The biggest one item may be, in KiB of its saved form (0: no limit).
    pub max_item_kb: usize,
    /// The biggest the storage's file may grow, in MiB (0: no limit).
    pub max_storage_mb: usize,
}

impl Default for Config {
    fn default() -> Self {
        parse(DEFAULT).expect("the default config parses")
    }
}

/// The settings in `text`; a key left out keeps its default, a wrong one is an error that
/// names it.
pub fn parse(text: &str) -> Result<Config, String> {
    let table: toml::Table = text.parse().map_err(|e| format!("{e}"))?;
    let mut c = Config {
        mode: Mode::Global,
        title: "Глобальне сховище".into(),
        permission: 0,
        max_types: 0,
        deposit_hotbar: false,
        blacklist: Vec::new(),
        save_every: 5,
        max_item_kb: 128,
        max_storage_mb: 16,
    };
    for (key, value) in &table {
        let bad = || format!("{key}: wrong value {value}");
        match key.as_str() {
            "mode" => {
                c.mode = match value.as_str() {
                    Some("global") => Mode::Global,
                    Some("player") => Mode::PerPlayer,
                    _ => return Err(format!("{key}: \"global\" or \"player\"")),
                }
            }
            "title" => c.title = value.as_str().ok_or_else(bad)?.to_owned(),
            "permission" => {
                c.permission = value
                    .as_integer()
                    .and_then(|n| u8::try_from(n).ok())
                    .filter(|n| *n <= 4)
                    .ok_or_else(bad)?
            }
            "max_types" => {
                c.max_types = value
                    .as_integer()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(bad)?
            }
            "deposit_hotbar" => c.deposit_hotbar = value.as_bool().ok_or_else(bad)?,
            "blacklist" => {
                c.blacklist = value
                    .as_array()
                    .ok_or_else(bad)?
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned).ok_or_else(bad))
                    .collect::<Result<_, _>>()?
            }
            "save_every" => {
                c.save_every = value
                    .as_integer()
                    .and_then(|n| u64::try_from(n).ok())
                    .filter(|n| *n >= 1)
                    .ok_or_else(bad)?
            }
            "max_item_kb" => {
                c.max_item_kb = value
                    .as_integer()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(bad)?
            }
            "max_storage_mb" => {
                c.max_storage_mb = value
                    .as_integer()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|n| *n <= 1000)
                    .ok_or_else(bad)?
            }
            _ => return Err(format!("unknown setting {key}")),
        }
    }
    Ok(c)
}

impl Config {
    /// [`Self::max_item_kb`] in bytes.
    pub fn max_item_bytes(&self) -> usize {
        self.max_item_kb.saturating_mul(1024)
    }
    /// [`Self::max_storage_mb`] in bytes.
    pub fn max_storage_bytes(&self) -> usize {
        self.max_storage_mb.saturating_mul(1024 * 1024)
    }

    /// Whether `id` (`minecraft:bedrock`) may go in.
    pub fn allowed(&self, id: &str) -> bool {
        !self.blacklist.iter().any(|b| match b.strip_suffix('*') {
            Some(prefix) => id.starts_with(prefix),
            None => b == id,
        })
    }
}
