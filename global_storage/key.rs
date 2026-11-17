// A message from a player's client: the "open storage" key of the serverkeys client mod.
fn global_storage_key(game: &mut crate::game::Game<'_>, player: u128, channel: &str, data: &[u8]) {
    crate::game::global_storage_glue::net_from_client(game, player, channel, data);
}
