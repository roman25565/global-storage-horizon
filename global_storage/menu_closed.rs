// A menu a native mod opened closed.
fn global_storage_menu_closed(uuid: u128, window: i32) {
    crate::game::global_storage_glue::menu_closed(uuid, window);
}
