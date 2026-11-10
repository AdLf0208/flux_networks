// The FE buffer a flux device declares (API 142).
fn observe_block_entity_energy(
    block: &str,
    energy: &mut Option<horizon_client::containers::ModEnergyBuffer>,
) {
    crate::game::fluxnetworks_state::on_block_entity_energy(block, energy);
}
