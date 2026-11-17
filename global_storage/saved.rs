// The world was saved: the storages are saved too.
fn global_storage_saved() {
    crate::game::global_storage_glue::flush();
}
