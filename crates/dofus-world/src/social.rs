use crate::WorldState;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// FriendsListMessage ID (polymorphic vector — built manually).
const FRIENDS_LIST_MSG_ID: u16 = 5813;
/// IgnoredListMessage ID (polymorphic vector — built manually).
const IGNORED_LIST_MSG_ID: u16 = 4244;

/// Handle FriendsGetListMessage — send the friends list.
/// For now, sends an empty list (no friends table populated yet).
pub async fn handle_friends_get_list(
    session: &mut Session,
    _state: &Arc<WorldState>,
    _account_id: i64,
) -> anyhow::Result<()> {
    // FriendsListMessage has a polymorphic vector (FriendInformations variants).
    // Send empty list for now.
    let mut w = BigEndianWriter::new();
    w.write_short(0); // friends_list count = 0
    session
        .send_raw(RawMessage {
            message_id: FRIENDS_LIST_MSG_ID,
            instance_id: 0,
            payload: w.into_data(),
        })
        .await?;
    Ok(())
}

/// Handle IgnoredGetListMessage — send the ignored list.
pub async fn handle_ignored_get_list(
    session: &mut Session,
    _state: &Arc<WorldState>,
    _account_id: i64,
) -> anyhow::Result<()> {
    // IgnoredListMessage has a polymorphic vector. Send empty.
    let mut w = BigEndianWriter::new();
    w.write_short(0); // ignored_list count = 0
    session
        .send_raw(RawMessage {
            message_id: IGNORED_LIST_MSG_ID,
            instance_id: 0,
            payload: w.into_data(),
        })
        .await?;
    Ok(())
}

/// Handle FriendAddRequestMessage — add a friend by name.
pub async fn handle_friend_add(
    session: &mut Session,
    state: &Arc<WorldState>,
    account_id: i64,
    name: &str,
) -> anyhow::Result<()> {
    // Look up target account by character name
    match repository::find_account_by_character_name(&state.pool, name).await? {
        Some(target_account_id) if target_account_id != account_id => {
            repository::add_friend(&state.pool, account_id, target_account_id).await?;
            tracing::info!(account_id, friend = name, "Friend added");
            // Re-send the full list
            handle_friends_get_list(session, state, account_id).await?;
        }
        _ => {
            tracing::debug!(account_id, friend = name, "Friend add: player not found");
        }
    }
    Ok(())
}

/// Handle FriendDeleteRequestMessage — remove a friend.
pub async fn handle_friend_delete(
    session: &mut Session,
    state: &Arc<WorldState>,
    account_id: i64,
    friend_account_id: i32,
) -> anyhow::Result<()> {
    repository::remove_friend(&state.pool, account_id, friend_account_id as i64).await?;
    session
        .send(&FriendDeleteResultMessage {
            success: true,
            name: String::new(),
        })
        .await?;
    Ok(())
}

/// Handle IgnoredAddRequestMessage — add to ignore list.
pub async fn handle_ignored_add(
    session: &mut Session,
    state: &Arc<WorldState>,
    account_id: i64,
    name: &str,
) -> anyhow::Result<()> {
    match repository::find_account_by_character_name(&state.pool, name).await? {
        Some(target_account_id) if target_account_id != account_id => {
            repository::add_ignored(&state.pool, account_id, target_account_id).await?;
            handle_ignored_get_list(session, state, account_id).await?;
        }
        _ => {}
    }
    Ok(())
}

/// Handle IgnoredDeleteRequestMessage — remove from ignore list.
pub async fn handle_ignored_delete(
    session: &mut Session,
    state: &Arc<WorldState>,
    account_id: i64,
    ignored_account_id: i32,
) -> anyhow::Result<()> {
    repository::remove_ignored(&state.pool, account_id, ignored_account_id as i64).await?;
    handle_ignored_get_list(session, state, account_id).await?;
    Ok(())
}
