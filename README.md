# Global Storage for Horizon

One big storage for players: put items in from your inventory, take them back, search by name.
Server-side only, players install nothing (works with the ordinary 26.3 client too).

## For players

The **I** key (the client mod "Server keys", [serverkeys](https://github.com/roman25565/serverkeys-horizon); rebindable in
Options → Mods → Server keys), `/gs` or `/storage` opens the storage.
`/gs <text>` opens it with a search right away.

**Terminal** (Horizon client with the "Server keys" mod 0.8+): a storage terminal is drawn
over the chest. The search field on top already has focus: just type, the list filters
instantly (on the client, no load on the server). Below is a scrollable 9×6 grid (wheel, arrows,
PageUp/PageDown, slider on the right), a short count in the corner (64, 1.2k, 35M), the exact one in
the tooltip. A sort button (count / A-Z / mod), a "Deposit" button (LMB everything, RMB only
what is already stored). LMB on an item takes a stack, RMB one item, Shift+click as much as fits. The inventory
below is the normal one: click or Shift+click your item to put it into storage. The server sends
the list on open and only changes after that (`global_storage/core/src/term.rs`, channels `global_storage:term`,
`global_storage:act`).

**An ordinary client** sees a chest: 5 rows of items, buttons at the bottom (pages, search via
a text window, sorting, "deposit all", info).

| What | How |
|---|---|
| Deposit a stack | click your item at the bottom (in the inventory under the menu) |
| Deposit all of a kind | double-click your item |
| Deposit everything | chest button: LMB everything from the inventory, RMB only what is already stored |
| Take a stack | LMB on a stored item |
| Take 1 | RMB |
| Take as much as fits | Shift+click |
| Stack to a hotbar slot | hover the item and press 1-9 |
| Search | spyglass: LMB opens a text field, RMB clears the search |
| Sorting | hopper: creative order (default), by count, by name, by mod/id |
| Pages | arrows; RMB on an arrow jumps to the first/last page |

Search understands Ukrainian and English names (`кругляк`, `cobble`), ids (`diamond_sword`),
anvil names and `@mod` (`@create` shows only Create items). Several words are matched together.

The count per slot is unlimited (a counter up to 18 quintillion); a slot shows up to 99, the exact
number and how many stacks it is are in the item's tooltip. Enchanted, renamed and damaged
items and shulker boxes with contents are stored separately and come back exactly as they were.

## For admins

Settings: `config/global_storage/global-storage.toml` (created on first start,
changes apply after a restart):

- `mode = "global"` one storage for everyone, `"player"` one per player;
- `permission` who may open it (0 everyone, 2 operators);
- `max_types` how many different kinds of items (0 = no limit);
- `deposit_hotbar` whether "deposit all" also takes the hotbar (off by default);
- `blacklist` forbidden items (`"minecraft:bedrock"`, `"somemod:*"`);
- `save_every` how often a changed storage is saved, in seconds.

Data: `world/global_storage/global.dat` (or `<uuid>.dat` per player), NBT like the
game itself uses; it is included in world backups. Written atomically (via `.tmp`), also on world
save and server stop. A damaged file is never overwritten: it is moved aside as
`.broken-<time>` and the error goes to the log.

## Code (in `global_storage/`)

- `core/` logic without the game: storage, search, sorting, menu layout, config; tests:
  `cargo test` in `core/` (don't leave Cargo.lock and target behind after testing: native-build fails).
  `core/data/names.tsv` item names (uk/en) from the 26.3 language files,
  `core/data/creative.tsv` the creative tab and order of every item from Horizon's creative export.
- `glue.rs` glue to the game: command, menu, inventory, files.
- the other `*.rs` are hooks (register_commands, perform_command, server_tick, menu_clicked,
  menu_closed, world_saved, server_stopping, net_from_client: the `global_storage:open` key
  from the `serverkeys:action` channel).

The mod is pinned to Horizon `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`, like the other server
mods, because it puts and takes stacks directly in the player's server-side records.

## License

MIT, see [LICENSE](LICENSE). A mod for [Horizon](https://github.com/horizon-rust/horizon) (MIT, Ludovic Marie and the Horizon contributors).

Unofficial project, not affiliated with Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Synergies AB.

The item names in `global_storage/core/data/names.tsv` come from the Minecraft 26.3 language files (© Mojang) and are not covered by the MIT license.
