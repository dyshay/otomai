use super::*;
use super::actors::{GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID_TEST, GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID_TEST};
use dofus_io::BigEndianWriter;
use dofus_network::codec::RawMessage;
use dofus_protocol::generated::types::EntityLook;
use tokio::sync::mpsc;

fn make_player(id: i64, name: &str) -> (MapPlayer, mpsc::UnboundedReceiver<RawMessage>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let player = MapPlayer {
        character_id: id,
        account_id: id,
        name: name.to_string(),
        entity_look: EntityLook {
            bones_id: 1,
            skins: vec![150],
            indexed_colors: vec![],
            scales: vec![100],
            subentities: vec![],
        },
        cell_id: 300,
        direction: 3,
        level: 1,
        breed: 8,
        sex: false,
        tx,
    };
    (player, rx)
}

#[tokio::test]
async fn join_map_adds_player() {
    let world = World::new();
    let (player, _rx) = make_player(1, "Alice");

    world.join_map(100, player).await;

    let players = world.get_players_on_map(100).await;
    assert_eq!(players.len(), 1);
    assert_eq!(players[0].name, "Alice");
}

#[tokio::test]
async fn join_map_broadcasts_to_existing() {
    let world = World::new();
    let (p1, mut rx1) = make_player(1, "Alice");
    let (p2, _rx2) = make_player(2, "Bob");

    world.join_map(100, p1).await;
    world.join_map(100, p2).await;

    // Alice should have received a ShowActor for Bob
    let msg = rx1.try_recv().unwrap();
    assert_eq!(msg.message_id, GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID_TEST);
}

#[tokio::test]
async fn leave_map_removes_player() {
    let world = World::new();
    let (p1, _rx1) = make_player(1, "Alice");
    let (p2, _rx2) = make_player(2, "Bob");

    world.join_map(100, p1).await;
    world.join_map(100, p2).await;
    world.leave_map(100, 1).await;

    let players = world.get_players_on_map(100).await;
    assert_eq!(players.len(), 1);
    assert_eq!(players[0].name, "Bob");
}

#[tokio::test]
async fn leave_map_broadcasts_remove() {
    let world = World::new();
    let (p1, _rx1) = make_player(1, "Alice");
    let (p2, mut rx2) = make_player(2, "Bob");

    world.join_map(100, p1).await;
    world.join_map(100, p2).await;

    // Drain the ShowActor that Bob's rx received when joining
    let _ = rx2.try_recv();

    world.leave_map(100, 1).await;

    // Bob should receive a RemoveElement for Alice
    let msg = rx2.try_recv().unwrap();
    assert_eq!(msg.message_id, GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID_TEST);
}

#[tokio::test]
async fn leave_last_player_cleans_up_map() {
    let world = World::new();
    let (p1, _rx1) = make_player(1, "Alice");

    world.join_map(100, p1).await;
    world.leave_map(100, 1).await;

    let players = world.get_players_on_map(100).await;
    assert!(players.is_empty());
}

#[tokio::test]
async fn empty_map_returns_empty_vec() {
    let world = World::new();
    let players = world.get_players_on_map(999).await;
    assert!(players.is_empty());
}

#[test]
fn build_character_informations_sets_fields() {
    let (player, _rx) = make_player(42, "TestHero");
    let info = build_character_informations(&player);

    assert_eq!(info.contextual_id, 42.0);
    assert_eq!(info.name, "TestHero");
    assert_eq!(info.account_id, 42);
    assert_eq!(info.look.bones_id, 1);
    assert_eq!(info.look.skins, vec![150]);
}

#[test]
fn show_actor_raw_has_correct_message_id() {
    let (player, _rx) = make_player(1, "Test");
    let raw = actors::build_show_actor_raw(&player);
    assert_eq!(raw.message_id, 3267);
    // Payload starts with TYPE_ID 5268 as big-endian u16
    assert_eq!(raw.payload[0], (5268 >> 8) as u8);
    assert_eq!(raw.payload[1], (5268 & 0xFF) as u8);
}

#[test]
fn remove_actor_raw_has_correct_message_id() {
    let raw = actors::build_remove_actor_raw(123);
    assert_eq!(raw.message_id, 6344);
    // Payload is a double (f64) representing 123.0
    assert_eq!(raw.payload.len(), 8);
}

#[test]
fn write_actors_empty() {
    let mut w = BigEndianWriter::new();
    write_actors(&mut w, &[]);
    let data = w.into_data();
    assert_eq!(data, vec![0, 0]);
}

#[tokio::test]
async fn broadcast_global_reaches_all_maps() {
    let world = World::new();
    let (p1, mut rx1) = make_player(1, "Alice");
    let (p2, mut rx2) = make_player(2, "Bob");

    world.join_map(100, p1).await;
    world.join_map(200, p2).await;

    // Drain ShowActor messages
    let _ = rx1.try_recv();
    let _ = rx2.try_recv();

    let raw = RawMessage {
        message_id: 9999,
        instance_id: 0,
        payload: vec![1, 2, 3],
    };
    world.broadcast_global(raw).await;

    // Both players on different maps should receive the broadcast
    assert!(rx1.try_recv().is_ok(), "Alice should receive global broadcast");
    assert!(rx2.try_recv().is_ok(), "Bob should receive global broadcast");
}

#[tokio::test]
async fn broadcast_to_map_only_targets_one_map() {
    let world = World::new();
    let (p1, mut rx1) = make_player(1, "Alice");
    let (p2, mut rx2) = make_player(2, "Bob");

    world.join_map(100, p1).await;
    world.join_map(200, p2).await;

    let _ = rx1.try_recv();
    let _ = rx2.try_recv();

    let raw = RawMessage {
        message_id: 9999,
        instance_id: 0,
        payload: vec![4, 5, 6],
    };
    world.broadcast_to_map(100, raw).await;

    assert!(rx1.try_recv().is_ok(), "Alice on map 100 should receive");
    assert!(rx2.try_recv().is_err(), "Bob on map 200 should NOT receive");
}

#[tokio::test]
async fn find_player_by_name_case_insensitive() {
    let world = World::new();
    let (p1, _rx1) = make_player(1, "Alice");

    world.join_map(100, p1).await;

    assert!(world.find_player_by_name("Alice").await.is_some());
    assert!(world.find_player_by_name("alice").await.is_some());
    assert!(world.find_player_by_name("ALICE").await.is_some());
    assert!(world.find_player_by_name("Bob").await.is_none());
}

#[tokio::test]
async fn find_player_by_id_across_maps() {
    let world = World::new();
    let (p1, _rx1) = make_player(1, "Alice");
    let (p2, _rx2) = make_player(2, "Bob");

    world.join_map(100, p1).await;
    world.join_map(200, p2).await;

    let found = world.find_player_by_id(2).await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Bob");

    assert!(world.find_player_by_id(999).await.is_none());
}
