// The server is stopping: the storages are saved.
fn global_storage_stopping() {
    crate::game::global_storage_glue::flush();
}
