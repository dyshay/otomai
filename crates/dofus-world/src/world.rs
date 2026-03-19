use dofus_io::{BigEndianWriter, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_protocol::generated::types::{
    ActorAlignmentInformations, ActorRestrictionsInformations, EntityDispositionInformations,
    EntityDispositionInformationsVariant, EntityLook, GameRolePlayCharacterInformations,
    HumanInformations, HumanInformationsVariant,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Message IDs for actor show/remove (polymorphic, sent as raw).
const GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID: u16 = 3267;
const GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID: u16 = 6344;

/// A player present on a map.
#[derive(Clone)]
pub struct MapPlayer {
    pub character_id: i64,
    pub account_id: i64,
    pub name: String,
    pub entity_look: EntityLook,
    pub cell_id: i16,
    pub direction: u8,
    pub level: i16,
    pub breed: u8,
    pub sex: bool,
    pub tx: mpsc::UnboundedSender<RawMessage>,
}

/// A single map instance holding its actors.
pub struct WorldMap {
    pub players: HashMap<i64, MapPlayer>, // character_id → player
}

impl WorldMap {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
        }
    }
}

/// Shared world state: all maps with their actors.
pub struct World {
    maps: RwLock<HashMap<i64, WorldMap>>, // map_id → WorldMap
}

impl World {
    pub fn new() -> Self {
        Self {
            maps: RwLock::new(HashMap::new()),
        }
    }

    /// Add a player to a map and broadcast their appearance to others.
    pub async fn join_map(&self, map_id: i64, player: MapPlayer) {
        let show_raw = build_show_actor_raw(&player);
        let mut maps = self.maps.write().await;
        let world_map = maps.entry(map_id).or_insert_with(WorldMap::new);

        // Broadcast to existing players on this map
        for (_, existing) in world_map.players.iter() {
            let _ = existing.tx.send(show_raw.clone());
        }

        world_map.players.insert(player.character_id, player);
    }

    /// Remove a player from a map and broadcast their departure.
    pub async fn leave_map(&self, map_id: i64, character_id: i64) {
        let mut maps = self.maps.write().await;
        if let Some(world_map) = maps.get_mut(&map_id) {
            world_map.players.remove(&character_id);

            let remove_raw = build_remove_actor_raw(character_id);
            for (_, existing) in world_map.players.iter() {
                let _ = existing.tx.send(remove_raw.clone());
            }

            // Clean up empty maps
            if world_map.players.is_empty() {
                maps.remove(&map_id);
            }
        }
    }

    /// Get all players currently on a map (for MapComplementary).
    pub async fn get_players_on_map(&self, map_id: i64) -> Vec<MapPlayer> {
        let maps = self.maps.read().await;
        maps.get(&map_id)
            .map(|m| m.players.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Update a player's cell position on their current map.
    pub async fn update_player_cell(&self, map_id: i64, character_id: i64, cell_id: i16) {
        let mut maps = self.maps.write().await;
        if let Some(world_map) = maps.get_mut(&map_id) {
            if let Some(player) = world_map.players.get_mut(&character_id) {
                player.cell_id = cell_id;
            }
        }
    }
}

/// Build a GameRolePlayShowActorMessage (ID 3267) as raw bytes.
/// The message contains a polymorphic actor — we write the TYPE_ID prefix
/// for GameRolePlayCharacterInformations (5268).
fn build_show_actor_raw(player: &MapPlayer) -> RawMessage {
    let info = build_character_informations(player);
    let mut w = BigEndianWriter::new();
    // Polymorphic: write type_id then serialize
    w.write_ushort(GameRolePlayCharacterInformations::TYPE_ID);
    info.serialize(&mut w);
    RawMessage {
        message_id: GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID,
        instance_id: 0,
        payload: w.into_data(),
    }
}

/// Build a GameContextRemoveElementMessage (ID 6344) as raw bytes.
fn build_remove_actor_raw(character_id: i64) -> RawMessage {
    let mut w = BigEndianWriter::new();
    w.write_double(character_id as f64);
    RawMessage {
        message_id: GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID,
        instance_id: 0,
        payload: w.into_data(),
    }
}

/// Build GameRolePlayCharacterInformations for a player on the map.
pub fn build_character_informations(player: &MapPlayer) -> GameRolePlayCharacterInformations {
    GameRolePlayCharacterInformations {
        contextual_id: player.character_id as f64,
        disposition: Box::new(EntityDispositionInformationsVariant::EntityDispositionInformations(
            EntityDispositionInformations {
                cell_id: player.cell_id,
                direction: player.direction,
            },
        )),
        look: player.entity_look.clone(),
        name: player.name.clone(),
        humanoid_info: Box::new(HumanInformationsVariant::HumanInformations(
            HumanInformations {
                restrictions: ActorRestrictionsInformations::default(),
                sex: player.sex,
                options: vec![],
            },
        )),
        account_id: player.account_id as i32,
        alignment_infos: ActorAlignmentInformations::default(),
    }
}

/// Build the actors portion for MapComplementaryInformationsDataMessage.
/// Writes the polymorphic actor array (count + type_id + data per actor).
pub fn write_actors(w: &mut BigEndianWriter, players: &[MapPlayer]) {
    w.write_short(players.len() as i16);
    for player in players {
        let info = build_character_informations(player);
        w.write_ushort(GameRolePlayCharacterInformations::TYPE_ID);
        info.serialize(w);
    }
}

/// Create a new broadcast channel for a player session.
pub fn new_broadcast_channel() -> (mpsc::UnboundedSender<RawMessage>, mpsc::UnboundedReceiver<RawMessage>) {
    mpsc::unbounded_channel()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(msg.message_id, GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID);
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
        assert_eq!(msg.message_id, GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID);
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
        let raw = build_show_actor_raw(&player);
        assert_eq!(raw.message_id, 3267);
        // Payload starts with TYPE_ID 5268 as big-endian u16
        assert_eq!(raw.payload[0], (5268 >> 8) as u8);
        assert_eq!(raw.payload[1], (5268 & 0xFF) as u8);
    }

    #[test]
    fn remove_actor_raw_has_correct_message_id() {
        let raw = build_remove_actor_raw(123);
        assert_eq!(raw.message_id, 6344);
        // Payload is a double (f64) representing 123.0
        assert_eq!(raw.payload.len(), 8);
    }

    #[test]
    fn write_actors_empty() {
        let mut w = BigEndianWriter::new();
        write_actors(&mut w, &[]);
        let data = w.into_data();
        // Should just be count=0 as i16
        assert_eq!(data, vec![0, 0]);
    }
}
