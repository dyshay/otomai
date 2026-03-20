//! Combat dispatch: fight initiation, turns, spells, movement in fight.

use super::session::PlayerSession;
use crate::{fight, quests, WorldState};
use dofus_database::repository;
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::registry::ProtocolMessage;
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn dispatch(
    msg: &ProtocolMessage,
    session: &mut Session,
    state: &Arc<WorldState>,
    ps: &mut PlayerSession,
) -> anyhow::Result<bool> {
    match msg {
        ProtocolMessage::GameRolePlayAttackMonsterRequestMessage(msg) => {
            if let Some(char_id) = ps.character_id {
                if let Some(map_id) = ps.map_id {
                    if let Some(character) = repository::get_character(&state.pool, char_id).await? {
                        ps.fight = fight::start_pve_fight(
                            session, state, &character, map_id, msg.monster_group_id, &ps.broadcast_tx,
                        ).await?;
                    }
                }
            }
        }
        ProtocolMessage::GameFightReadyMessage(_) => {
            if let Some(ref mut f) = ps.fight {
                fight::handle_fight_ready(session, f).await?;
            }
        }
        ProtocolMessage::GameFightPlacementPositionRequestMessage(msg) => {
            if let Some(ref mut f) = ps.fight {
                if let Some(char_id) = ps.character_id {
                    fight::handle_placement_position(session, f, char_id as f64, msg.cell_id).await?;
                }
            }
        }
        ProtocolMessage::GameFightTurnFinishMessage(_) => {
            if let Some(ref mut f) = ps.fight {
                fight::turns::handle_turn_finish(session, f).await?;
                check_fight_end(session, state, ps).await?;
            }
        }
        ProtocolMessage::GameFightTurnReadyMessage(_) => {}
        ProtocolMessage::GameActionFightCastRequestMessage(msg) => {
            if let Some(ref mut f) = ps.fight {
                if let Some(char_id) = ps.character_id {
                    fight::spells::handle_spell_cast(
                        session, state, f, char_id as f64, msg.spell_id, msg.cell_id,
                    ).await?;
                    check_fight_end(session, state, ps).await?;
                }
            }
        }
        ProtocolMessage::GameMapMovementRequestMessage(msg) if ps.fight.is_some() => {
            if let Some(ref mut f) = ps.fight {
                if let Some(char_id) = ps.character_id {
                    fight::turns::handle_fight_movement(session, f, char_id as f64, &msg.key_movements).await?;
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

async fn check_fight_end(
    session: &mut Session,
    state: &Arc<WorldState>,
    ps: &mut PlayerSession,
) -> anyhow::Result<()> {
    let should_end = ps.fight
        .as_ref()
        .map(|f| f.phase == fight::state::FightPhase::Ended || f.should_end())
        .unwrap_or(false);

    if !should_end {
        return Ok(());
    }

    if let Some(f) = ps.fight.take() {
        let map_id = f.map_id;
        let won = f.challengers_won();
        if let Some(char_id) = ps.character_id {
            if let Some(character) = repository::get_character(&state.pool, char_id).await? {
                fight::handle_fight_end(session, state, &f, &character, &ps.broadcast_tx).await?;
                ps.map_id = Some(map_id);

                if won {
                    let killed_ids: Vec<i32> = f.fighters.iter()
                        .filter(|fi| !fi.is_player && !fi.is_alive)
                        .map(|fi| fi.monster_id)
                        .collect();
                    quests::check_defeat_monster_objectives(session, state, char_id, &killed_ids, map_id).await?;
                }
            }
        }
    }

    Ok(())
}
