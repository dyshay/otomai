use dofus_network::codec::RawMessage;
use dofus_protocol::generated::types::EntityLook;
use std::collections::HashMap;
use tokio::sync::{mpsc, RwLock};

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
        let show_raw = super::actors::build_show_actor_raw(&player);
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

            let remove_raw = super::actors::build_remove_actor_raw(character_id);
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

    /// Broadcast a raw message to ALL players in the world (global channels).
    pub async fn broadcast_global(&self, raw: RawMessage) {
        let maps = self.maps.read().await;
        for world_map in maps.values() {
            for player in world_map.players.values() {
                let _ = player.tx.send(raw.clone());
            }
        }
    }

    /// Broadcast a raw message to all players on a specific map.
    pub async fn broadcast_to_map(&self, map_id: i64, raw: RawMessage) {
        let maps = self.maps.read().await;
        if let Some(world_map) = maps.get(&map_id) {
            for player in world_map.players.values() {
                let _ = player.tx.send(raw.clone());
            }
        }
    }

    /// Find a player by character name across all maps.
    pub async fn find_player_by_name(&self, name: &str) -> Option<MapPlayer> {
        let maps = self.maps.read().await;
        for world_map in maps.values() {
            for player in world_map.players.values() {
                if player.name.eq_ignore_ascii_case(name) {
                    return Some(player.clone());
                }
            }
        }
        None
    }

    /// Find a player by character_id across all maps.
    pub async fn find_player_by_id(&self, character_id: i64) -> Option<MapPlayer> {
        let maps = self.maps.read().await;
        for world_map in maps.values() {
            if let Some(player) = world_map.players.get(&character_id) {
                return Some(player.clone());
            }
        }
        None
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

/// Create a new broadcast channel for a player session.
pub fn new_broadcast_channel() -> (mpsc::UnboundedSender<RawMessage>, mpsc::UnboundedReceiver<RawMessage>) {
    mpsc::unbounded_channel()
}
