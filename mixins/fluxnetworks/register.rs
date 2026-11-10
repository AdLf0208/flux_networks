// `/fluxnetworks info|energy|network|priority` (API 8).
fn observe_register_commands(commands: &mut Vec<NativeCommand>) {
    crate::game::fluxnetworks_state::on_register_commands(commands);
}
