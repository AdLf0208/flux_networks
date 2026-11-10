// After the level's tick: every network of this level runs one cycle.
fn observe_level_tick_post(game: &mut crate::game::Game<'_>, dimension: &str) {
    crate::game::fluxnetworks_state::on_level_tick_post(game, dimension);
}
