// Each server tick: open the storage menus asked for, save a changed storage now and then.
fn global_storage_tick(game: &mut crate::game::Game<'_>) {
    crate::game::global_storage_glue::tick(game);
}
