// One server tick of a flux device: it queues itself for the network pass.
fn observe_block_entity_tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
) {
    crate::game::fluxnetworks_state::on_block_entity_tick(game, pos, entity);
}
