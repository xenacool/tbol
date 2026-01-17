use ghx_grid::cartesian::coordinates::Cartesian3D;
use ghx_grid::cartesian::grid::CartesianGrid;
use ghx_grid::grid::{GridData, GridIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type StringPath = String;
pub type StringContent = String;
pub type PaletteIndex = u32;
pub type RoomId = u32;

/// Runtime island state - loaded from individual files, not serialized as a whole
#[derive(Clone, Debug)]
pub struct IslandData {
    pub island: Island,
    pub rooms: Vec<Room>,
}

/// Core island configuration - serialized to RON by editor
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Island {
    pub dock_room_id: RoomId,
    pub name: StringContent,
    pub description: StringContent,
}

/// Room definition - serialized to RON by editor
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Room {
    pub room_id: RoomId,
    /// World position (for adjacency checks)
    pub pos_x: i64,
    pub pos_y: i64,
    pub pos_z: i64,
    /// Grid extents
    pub extent_x: u32,
    pub extent_y: u32,
    pub extent_z: u32,
    /// Looping per axis
    pub looping_x: bool,
    pub looping_y: bool,
    pub looping_z: bool,
    /// Tile data: grid index -> tile
    pub tiles: HashMap<GridIndex, TileData>,
}

/// Entity spawn point - serialized to RON by editor
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EntitySpawn {
    pub entity_type: StringContent,
    pub room_id: RoomId,
    pub grid_index: GridIndex,
    /// Luau-defined properties serialized as strings
    pub properties: HashMap<StringContent, StringContent>,
}

/// Minimal tile data - Luau defines semantics via register_tile_field
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum TileData {
    None,
    /// Palette index references GLTF model
    Tile(PaletteIndex),
    /// Door connects to another room (no adjacency needed)
    Door(PaletteIndex, RoomId),
}

impl IslandData {
    pub fn new(island: Island, rooms: Vec<Room>) -> Self {
        Self { island, rooms }
    }

    /// Check if two rooms are physically adjacent (share a face)
    /// This allows navigation without explicit doors (haunted house mechanics)
    pub fn rooms_are_adjacent(&self, room_a_id: RoomId, room_b_id: RoomId) -> bool {
        if room_a_id == room_b_id {
            return false;
        }

        let room_a = self.rooms.iter().find(|r| r.room_id == room_a_id);
        let room_b = self.rooms.iter().find(|r| r.room_id == room_b_id);

        match (room_a, room_b) {
            (Some(a), Some(b)) => Room::are_adjacent(a, b),
            _ => false,
        }
    }
}

impl Room {
    /// Check if two rooms share a face (are physically adjacent)
    pub fn are_adjacent(a: &Room, b: &Room) -> bool {
        let a_min_x = a.pos_x;
        let a_max_x = a.pos_x + a.extent_x as i64;
        let a_min_y = a.pos_y;
        let a_max_y = a.pos_y + a.extent_y as i64;
        let a_min_z = a.pos_z;
        let a_max_z = a.pos_z + a.extent_z as i64;

        let b_min_x = b.pos_x;
        let b_max_x = b.pos_x + b.extent_x as i64;
        let b_min_y = b.pos_y;
        let b_max_y = b.pos_y + b.extent_y as i64;
        let b_min_z = b.pos_z;
        let b_max_z = b.pos_z + b.extent_z as i64;

        // Check if they share a face on any axis
        let x_adjacent = (a_max_x == b_min_x || b_max_x == a_min_x)
            && !(a_max_y <= b_min_y || b_max_y <= a_min_y)
            && !(a_max_z <= b_min_z || b_max_z <= a_min_z);

        let y_adjacent = (a_max_y == b_min_y || b_max_y == a_min_y)
            && !(a_max_x <= b_min_x || b_max_x <= a_min_x)
            && !(a_max_z <= b_min_z || b_max_z <= a_min_z);

        let z_adjacent = (a_max_z == b_min_z || b_max_z == a_min_z)
            && !(a_max_x <= b_min_x || b_max_x <= a_min_x)
            && !(a_max_y <= b_min_y || b_max_y <= a_min_y);

        x_adjacent || y_adjacent || z_adjacent
    }

    pub fn create_grid(&self) -> GridData<Cartesian3D, TileData, CartesianGrid<Cartesian3D>> {
        let grid = CartesianGrid::new_cartesian_3d(
            self.extent_x,
            self.extent_y,
            self.extent_z,
            self.looping_x,
            self.looping_y,
            self.looping_z,
        );
        let mut grid_data = grid.new_grid_data(TileData::None);
        for (index, tile) in &self.tiles {
            grid_data.set(*index, tile.clone());
        }
        grid_data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghx_grid::grid::Grid;

    fn create_test_island() -> Island {
        Island {
            dock_room_id: 1,
            name: "Test Island".to_string(),
            description: "A test island".to_string(),
        }
    }

    fn create_test_room() -> Room {
        let mut tiles = HashMap::new();
        tiles.insert(0, TileData::Tile(0));
        tiles.insert(1, TileData::Tile(1));

        Room {
            room_id: 1,
            pos_x: 0,
            pos_y: 0,
            pos_z: 0,
            extent_x: 3,
            extent_y: 3,
            extent_z: 3,
            looping_x: false,
            looping_y: false,
            looping_z: false,
            tiles,
        }
    }

    #[test]
    fn test_create_grid_from_room() {
        let room = create_test_room();
        let grid_data = room.create_grid();
        assert_eq!(grid_data.grid().total_size(), 27); // 3x3x3
    }

    #[test]
    fn test_room_adjacency_x_axis() {
        let room_a = Room {
            room_id: 1,
            pos_x: 0,
            pos_y: 0,
            pos_z: 0,
            extent_x: 5,
            extent_y: 5,
            extent_z: 5,
            looping_x: false,
            looping_y: false,
            looping_z: false,
            tiles: HashMap::new(),
        };

        let room_b = Room {
            room_id: 2,
            pos_x: 5,
            pos_y: 0,
            pos_z: 0,
            extent_x: 5,
            extent_y: 5,
            extent_z: 5,
            looping_x: false,
            looping_y: false,
            looping_z: false,
            tiles: HashMap::new(),
        };

        assert!(Room::are_adjacent(&room_a, &room_b));
    }

    #[test]
    fn test_room_adjacency_not_adjacent() {
        let room_a = Room {
            room_id: 1,
            pos_x: 0,
            pos_y: 0,
            pos_z: 0,
            extent_x: 5,
            extent_y: 5,
            extent_z: 5,
            looping_x: false,
            looping_y: false,
            looping_z: false,
            tiles: HashMap::new(),
        };

        let room_b = Room {
            room_id: 2,
            pos_x: 10,
            pos_y: 0,
            pos_z: 0,
            extent_x: 5,
            extent_y: 5,
            extent_z: 5,
            looping_x: false,
            looping_y: false,
            looping_z: false,
            tiles: HashMap::new(),
        };

        assert!(!Room::are_adjacent(&room_a, &room_b));
    }

    #[test]
    fn test_rooms_are_adjacent_through_island_data() {
        let island = create_test_island();
        let room_a = Room {
            room_id: 1,
            pos_x: 0,
            pos_y: 0,
            pos_z: 0,
            extent_x: 5,
            extent_y: 5,
            extent_z: 5,
            looping_x: false,
            looping_y: false,
            looping_z: false,
            tiles: HashMap::new(),
        };

        let room_b = Room {
            room_id: 2,
            pos_x: 5,
            pos_y: 0,
            pos_z: 0,
            extent_x: 5,
            extent_y: 5,
            extent_z: 5,
            looping_x: false,
            looping_y: false,
            looping_z: false,
            tiles: HashMap::new(),
        };

        let island_data = IslandData::new(island, vec![room_a, room_b]);
        assert!(island_data.rooms_are_adjacent(1, 2));
        assert!(!island_data.rooms_are_adjacent(1, 999));
    }

    #[test]
    fn test_ron_serialization_room() {
        let room = create_test_room();
        let serialized = ron::to_string(&room).unwrap();
        let deserialized: Room = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.room_id, 1);
        assert_eq!(deserialized.tiles.len(), 2);
    }

    #[test]
    fn test_ron_serialization_island() {
        let island = create_test_island();
        let serialized = ron::to_string(&island).unwrap();
        let deserialized: Island = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.name, "Test Island");
    }

    #[test]
    fn test_ron_serialization_entity_spawn() {
        let mut properties = HashMap::new();
        properties.insert("health".to_string(), "100".to_string());

        let spawn = EntitySpawn {
            entity_type: "npc_basic".to_string(),
            room_id: 1,
            grid_index: 5,
            properties,
        };

        let serialized = ron::to_string(&spawn).unwrap();
        let deserialized: EntitySpawn = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.entity_type, "npc_basic");
        assert_eq!(deserialized.properties.get("health").unwrap(), "100");
    }
}
