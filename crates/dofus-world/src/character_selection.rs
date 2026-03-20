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

/// Breed skin IDs — maps (breed_id, sex) to the base skin.
/// From the Breed D2O data (maleLook/femaleLook fields).
/// Format: BREED_SKINS[breed_id - 1] = (male_skin, female_skin)
const BREED_SKINS: [(i16, i16); 18] = [
    (10, 20),   // 1  Feca
    (30, 40),   // 2  Osamodas
    (50, 60),   // 3  Enutrof
    (70, 80),   // 4  Sram
    (90, 100),  // 5  Xelor
    (110, 120), // 6  Ecaflip
    (130, 140), // 7  Eniripsa
    (150, 160), // 8  Iop
    (170, 180), // 9  Cra
    (190, 200), // 10 Sadida
    (210, 220), // 11 Sacrieur
    (230, 240), // 12 Pandawa
    (250, 260), // 13 Roublard
    (270, 280), // 14 Zobal
    (290, 300), // 15 Steamer (Foggernauts)
    (310, 320), // 16 Eliotrope
    (330, 340), // 17 Huppermage
    (350, 360), // 18 Ouginak
];

/// Build a CharacterBaseInformations from a DB Character.
fn character_to_base_info(c: &Character) -> CharacterBaseInformations {
    // Encode colors as indexed_colors: (colorIndex << 24) | (rgb & 0xFFFFFF)
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

    // bonesId: 1 = male, 2 = female
    let bones_id: i16 = if c.sex == 0 { 1 } else { 2 };

    // Breed-specific skin
    let breed_idx = (c.breed_id as usize).saturating_sub(1).min(BREED_SKINS.len() - 1);
    let skin_id = if c.sex == 0 {
        BREED_SKINS[breed_idx].0
    } else {
        BREED_SKINS[breed_idx].1
    };

    CharacterBaseInformations {
        id: c.id,
        name: c.name.clone(),
        level: c.level as i16,
        entity_look: EntityLook {
            bones_id,
            skins: vec![skin_id],
            indexed_colors,
            scales: vec![100], // 100 = 1.0x scale
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

    // Build CharactersListMessage manually with minimal EntityLook for testing
    let mut writer = BigEndianWriter::new();
    writer.write_short(base_infos.len() as i16);
    for char_info in &base_infos {
        writer.write_ushort(CharacterBaseInformations::TYPE_ID);
        // AbstractCharacterInformation: id
        writer.write_var_long(char_info.id);
        // CharacterBasicMinimalInformations: name
        writer.write_utf(&char_info.name);
        // CharacterMinimalInformations: level
        writer.write_var_short(char_info.level);
        // CharacterMinimalPlusLookInformations: entityLook + breed
        char_info.entity_look.serialize(&mut writer);
        writer.write_byte(char_info.breed);
        // CharacterBaseInformations: sex
        writer.write_boolean(char_info.sex);
    }
    writer.write_boolean(false); // hasStartupActions
    let payload = writer.into_data();

    tracing::debug!(
        account_id,
        payload_len = payload.len(),
        first_bytes = ?&payload[..payload.len().min(40)],
        "CharactersListMessage payload"
    );
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

    // Messages required after selection (from GinyCore reference)
    session
        .send(&NotificationListMessage {
            flags: vec![0x7FFFFFFF],
        })
        .await?;

    session
        .send(&CharacterCapabilitiesMessage {
            guild_emblem_symbol_categories: 4095,
        })
        .await?;

    session
        .send(&SequenceNumberRequestMessage {})
        .await?;

    Ok(true)
}
