use crate::WorldState;
use dofus_database::models::Character;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::types::CharacterBaseInformations;
use dofus_protocol::generated::types::EntityLook;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Build a CharacterBaseInformations from a DB Character.
fn character_to_base_info(c: &Character) -> CharacterBaseInformations {
    let indexed_colors: Vec<i32> = c
        .colors
        .as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    v.as_i64().map(|color| {
                        (((i + 1) as i32) << 24) | ((color as i32) & 0x00FFFFFF)
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let bones_id: i16 = 1;
    let skin_id: i16 = (c.breed_id * 10 + c.sex + 1) as i16;

    CharacterBaseInformations {
        id: c.id,
        name: c.name.clone(),
        level: c.level as i16,
        entity_look: EntityLook {
            bones_id,
            skins: vec![skin_id],
            indexed_colors,
            scales: vec![125],
            subentities: vec![],
        },
        breed: c.breed_id as u8,
        sex: c.sex != 0,
    }
}

/// Build the raw payload for CharactersListMessage with polymorphic serialization.
fn build_characters_list_payload(
    characters: &[CharacterBaseInformations],
    has_startup_actions: bool,
) -> Vec<u8> {
    let mut writer = BigEndianWriter::new();
    writer.write_short(characters.len() as i16);
    for char_info in characters {
        writer.write_ushort(CharacterBaseInformations::TYPE_ID);
        char_info.serialize(&mut writer);
    }
    writer.write_boolean(has_startup_actions);
    writer.into_data()
}

/// Handle CharactersListRequestMessage: query DB and send character list.
pub async fn handle_characters_list_request(
    session: &mut Session,
    state: &Arc<WorldState>,
    account_id: i64,
) -> anyhow::Result<()> {
    let db_characters = repository::list_characters(&state.pool, account_id).await?;
    let base_infos: Vec<CharacterBaseInformations> =
        db_characters.iter().map(character_to_base_info).collect();

    tracing::debug!(account_id, count = base_infos.len(), "Sending characters list");

    let payload = build_characters_list_payload(&base_infos, false);
    session
        .send_raw(RawMessage {
            message_id: CharactersListMessage::MESSAGE_ID,
            instance_id: 0,
            payload,
        })
        .await?;

    Ok(())
}

/// Handle CharacterNameSuggestionRequestMessage: generate a random name.
pub async fn handle_name_suggestion(session: &mut Session) -> anyhow::Result<()> {
    use rand::{Rng, SeedableRng};

    const PREFIXES: &[&str] = &[
        "Oto", "Eni", "Sra", "Osa", "Xel", "Enu", "Sad", "Eca", "Fog",
        "Ior", "Pan", "Rogue", "Elio", "Hup", "Brak", "Alma", "Amu",
    ];
    const SUFFIXES: &[&str] = &[
        "mai", "ripsa", "mus", "lor", "othep", "bur", "idas", "flip",
        "nox", "gard", "zel", "ryn", "vax", "dor", "kel", "mira",
    ];

    let mut rng = rand::rngs::StdRng::from_entropy();
    let name = format!(
        "{}{}",
        PREFIXES[rng.gen_range(0..PREFIXES.len())],
        SUFFIXES[rng.gen_range(0..SUFFIXES.len())]
    );

    session
        .send(&CharacterNameSuggestionSuccessMessage { suggestion: name })
        .await?;

    Ok(())
}

/// Handle CharacterCreationRequestMessage: create character in DB.
pub async fn handle_character_creation(
    session: &mut Session,
    state: &Arc<WorldState>,
    account_id: i64,
    msg: &CharacterCreationRequestMessage,
) -> anyhow::Result<()> {
    // Validate name
    if msg.name.is_empty() || msg.name.len() > 20 {
        session
            .send(&CharacterCreationResultMessage { result: 4 }) // ERR_INVALID_NAME
            .await?;
        return Ok(());
    }

    // Check name uniqueness
    if repository::character_name_exists(&state.pool, &msg.name).await? {
        session
            .send(&CharacterCreationResultMessage { result: 1 }) // ERR_NAME_ALREADY_EXISTS
            .await?;
        return Ok(());
    }

    let colors = serde_json::json!(msg.colors.to_vec());
    let sex = if msg.sex { 1 } else { 0 };

    match repository::create_character(
        &state.pool,
        account_id,
        &msg.name,
        msg.breed as i32,
        sex,
        &colors,
    )
    .await
    {
        Ok(character) => {
            tracing::info!(account_id, character_id = character.id, name = %character.name, "Character created");
            // OK result
            session
                .send(&CharacterCreationResultMessage { result: 0 })
                .await?;
            // Resend characters list
            handle_characters_list_request(session, state, account_id).await?;
        }
        Err(e) => {
            tracing::warn!(account_id, error = %e, "Character creation failed");
            session
                .send(&CharacterCreationResultMessage { result: 2 }) // ERR_TOO_MANY_CHARACTERS
                .await?;
        }
    }

    Ok(())
}

/// Handle CharacterSelectionMessage: validate ownership and send success.
pub async fn handle_character_selection(
    session: &mut Session,
    state: &Arc<WorldState>,
    account_id: i64,
    character_id: i64,
) -> anyhow::Result<bool> {
    let character = match repository::get_character_for_account(
        &state.pool,
        character_id,
        account_id,
    )
    .await?
    {
        Some(c) => c,
        None => {
            tracing::warn!(account_id, character_id, "Character not found or not owned");
            session.send(&CharacterSelectedErrorMessage {}).await?;
            return Ok(false);
        }
    };

    let base_info = character_to_base_info(&character);

    tracing::info!(account_id, character_id, name = %character.name, "Character selected");

    session
        .send(&CharacterSelectedSuccessMessage {
            infos: base_info,
            is_collecting_stats: false,
        })
        .await?;

    Ok(true)
}
