//! Quest tracking: list, start, step completion.

use super::d2o::{get_first_step_id, get_step_objectives};
use crate::WorldState;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::types::QuestActiveInformations;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

const QUEST_LIST_MSG_ID: u16 = 7788;

pub async fn handle_quest_list(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
) -> anyhow::Result<()> {
    let all_quests = repository::get_character_quests(&state.pool, character_id).await?;

    let finished_ids: Vec<i16> = all_quests.iter().filter(|q| q.status == 1).map(|q| q.quest_id as i16).collect();
    let finished_counts: Vec<i16> = vec![1i16; finished_ids.len()];
    let active: Vec<&dofus_database::models::CharacterQuest> = all_quests.iter().filter(|q| q.status == 0).collect();

    let mut w = BigEndianWriter::new();
    w.write_short(finished_ids.len() as i16);
    for id in &finished_ids { w.write_var_short(*id); }
    w.write_short(finished_counts.len() as i16);
    for c in &finished_counts { w.write_var_short(*c); }
    w.write_short(active.len() as i16);
    for quest in &active {
        w.write_ushort(QuestActiveInformations::TYPE_ID);
        QuestActiveInformations { quest_id: quest.quest_id as i16 }.serialize(&mut w);
    }
    w.write_short(0); // reinit_done_quests_ids

    session
        .send_raw(RawMessage { message_id: QUEST_LIST_MSG_ID, instance_id: 0, payload: w.into_data() })
        .await?;
    Ok(())
}

pub async fn start_quest(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    quest_id: i32,
) -> anyhow::Result<()> {
    let step_id = get_first_step_id(state, quest_id).await.unwrap_or(0);
    let objectives = get_step_objectives(state, step_id).await;

    repository::start_quest(&state.pool, character_id, quest_id, step_id, &objectives).await?;

    session.send(&QuestStartedMessage { quest_id: quest_id as i16 }).await?;
    session.send(&QuestStepStartedMessage { quest_id: quest_id as i16, step_id: step_id as i16 }).await?;

    tracing::info!(character_id, quest_id, step_id, "Quest started");
    Ok(())
}

pub(super) async fn complete_step_if_done(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    quest_id: i32,
    step_id: i32,
    objectives: &serde_json::Value,
) -> anyhow::Result<()> {
    let all_done = objectives.as_array()
        .map(|arr| arr.iter().all(|o| o.get("completed").and_then(|v| v.as_bool()).unwrap_or(false)))
        .unwrap_or(false);

    if all_done {
        super::rewards::award_step_rewards(session, state, character_id, step_id).await?;

        session.send(&QuestStepValidatedMessage { quest_id: quest_id as i16, step_id: step_id as i16 }).await?;

        match super::d2o::get_next_step_id(state, quest_id, step_id).await {
            Some(next_id) => {
                let next_objectives = get_step_objectives(state, next_id).await;
                repository::update_quest_step(&state.pool, character_id, quest_id, next_id, &next_objectives).await?;
                session.send(&QuestStepStartedMessage { quest_id: quest_id as i16, step_id: next_id as i16 }).await?;
            }
            None => {
                repository::complete_quest(&state.pool, character_id, quest_id).await?;
                session.send(&QuestValidatedMessage { quest_id: quest_id as i16 }).await?;
            }
        }
    } else {
        repository::update_quest_step(&state.pool, character_id, quest_id, step_id, objectives).await?;
    }

    Ok(())
}
