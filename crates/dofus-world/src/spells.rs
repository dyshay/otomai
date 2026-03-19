use crate::WorldState;
use dofus_database::repository;
use dofus_network::session::Session;
use dofus_protocol::generated::types::SpellItem;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Send SpellListMessage — all spells for a character.
pub async fn send_spell_list(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
) -> anyhow::Result<()> {
    let db_spells = repository::list_spells(&state.pool, character_id).await?;

    let spells: Vec<SpellItem> = db_spells
        .iter()
        .map(|s| SpellItem {
            spell_id: s.spell_id,
            spell_level: s.level as i16,
        })
        .collect();

    session
        .send(&SpellListMessage {
            spell_previsualization: false,
            spells,
        })
        .await?;
    Ok(())
}
