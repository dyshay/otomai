use crate::{character_selection, WorldState};
use dofus_database::repository;
use dofus_io::DofusMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::messages::game_approach::*;
use dofus_protocol::generated::messages::secure::*;
use dofus_protocol::generated::messages::subscription::*;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Validate the ticket from AuthenticationTicketMessage and send acceptance + capabilities.
/// Returns the account_id on success, None if ticket is invalid.
pub async fn handle_ticket(
    session: &mut Session,
    state: &Arc<WorldState>,
    ticket_msg: &AuthenticationTicketMessage,
) -> anyhow::Result<Option<i64>> {
    let peer = session.peer_addr()?;

    // Trim null bytes from AES padding (NullPad: plaintext padded with 0x00 to 16-byte boundary)
    let clean_ticket = ticket_msg.ticket.trim_end_matches('\0');

    let ticket = match repository::consume_ticket(&state.pool, clean_ticket).await? {
        Some(t) => t,
        None => {
            tracing::warn!(%peer, "Invalid or expired ticket");
            session.send(&AuthenticationTicketRefusedMessage {}).await?;
            return Ok(None);
        }
    };

    tracing::info!(%peer, account_id = ticket.account_id, "Ticket validated");

    // Send acceptance
    session.send(&AuthenticationTicketAcceptedMessage {}).await?;

    // AccountInformationsUpdateMessage (subscription end date — 3 years from now)
    let sub_end = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64() + (3.0 * 365.25 * 24.0 * 3600.0);
    session.send(&AccountInformationsUpdateMessage {
        subscription_end_date: sub_end,
    }).await?;

    // Send account capabilities (all breeds visible/available)
    session.send(&AccountCapabilitiesMessage {
        tutorial_available: false,
        can_create_new_character: true,
        account_id: ticket.account_id as i32,
        breeds_visible: 0x3FFF, // all 14 breeds
        breeds_available: 0x3FFF,
        status: 0,
    }).await?;

    // Send BasicTimeMessage
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    session.send(&BasicTimeMessage {
        timestamp: (now.as_millis() as f64),
        timezone_offset: 0,
    }).await?;

    // Send ServerSettingsMessage
    session.send(&ServerSettingsMessage {
        is_mono_account: false,
        has_free_autopilot: false,
        lang: ticket_msg.lang.clone(),
        community: 0,
        game_type: 0,
        arena_leave_ban_time: 0,
        item_max_level: 200,
    }).await?;

    // ServerOptionalFeaturesMessage (empty features)
    session.send(&ServerOptionalFeaturesMessage {
        features: vec![],
    }).await?;

    // ServerSessionConstantsMessage (empty — polymorphic, send raw)
    session.send_raw(dofus_network::codec::RawMessage {
        message_id: ServerSessionConstantsMessage::MESSAGE_ID,
        instance_id: 0,
        payload: vec![0, 0], // count=0
    }).await?;

    // TrustStatusMessage
    session.send(&TrustStatusMessage {
        trusted: true,
        certified: true,
    }).await?;

    // Proactively send character list so the client shows selection screen
    // instead of defaulting to creation (AccountCapabilitiesMessage triggers CharacterCreationStart)
    character_selection::handle_characters_list_request(session, state, ticket.account_id).await?;

    Ok(Some(ticket.account_id))
}
