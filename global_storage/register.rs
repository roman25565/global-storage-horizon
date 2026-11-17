// Global Storage's commands (/gs, /storage) in every player's command tree.
fn global_storage_register(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    crate::game::global_storage_glue::commands(commands);
}
