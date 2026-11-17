// A click in a menu a native mod opened: ours, when it is the storage's.
fn global_storage_menu_clicked(
    game: &mut crate::game::Game<'_>,
    uuid: u128,
    window: i32,
    slot: i32,
    button: i32,
    mode: i32,
) {
    crate::game::global_storage_glue::menu_clicked(game, uuid, window, slot, button, mode);
}
