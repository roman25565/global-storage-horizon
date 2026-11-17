// A player's command on the server: Global Storage's, when it is one.
fn global_storage_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    if !*ran {
        crate::game::global_storage_glue::perform(game, player, command, ran);
    }
}
