use crate::mechanics::{
    EntitySpawn, Island as MechanicsIsland, IslandData as MechanicsIslandData, Room,
};
use mlua::{Error as LuaError, Function, Lua, Table, UserData, Value};
use path_security::{validate_filename, validate_path};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum DefaultValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct FieldOptions {
    pub default: Option<DefaultValue>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub values: Option<Vec<String>>,
    pub keys: Option<String>,
    pub value_type: Option<String>,
    pub item_type: Option<String>,
    pub schema: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct FieldRegistration {
    pub field_name: String,
    pub field_type: String,
    pub options: FieldOptions,
}

#[derive(Debug, Default)]
pub struct IslandData {
    pub tile_layers: Vec<String>,
    pub entity_layers: Vec<String>,
    pub tile_fields: HashMap<String, Vec<FieldRegistration>>,
    pub entity_fields: HashMap<String, Vec<FieldRegistration>>,
    // Runtime loaded data
    pub island_config: Option<MechanicsIsland>,
    pub rooms: Vec<Room>,
    pub entity_spawns: Vec<EntitySpawn>,
    pub gltf_registry: HashMap<String, PathBuf>,
    pub base_path: PathBuf,
    pub room_process_fns: HashMap<u32, mlua::RegistryKey>,
    pub room_physics_process_fns: HashMap<u32, mlua::RegistryKey>,
    // Process callbacks (cannot be cloned due to RegistryKey)
    pub process_fn: Option<mlua::RegistryKey>,
    pub physics_process_fn: Option<mlua::RegistryKey>,
}

#[derive(Clone)]
pub struct Island {
    data: Arc<Mutex<IslandData>>,
}

impl Island {
    pub fn new() -> Self {
        Island {
            data: Arc::new(Mutex::new(IslandData {
                base_path: PathBuf::from("tbol_vanilla"),
                ..Default::default()
            })),
        }
    }

    pub fn get_tile_layers(&self) -> Vec<String> {
        self.data.lock().unwrap().tile_layers.clone()
    }

    pub fn get_entity_layers(&self) -> Vec<String> {
        self.data.lock().unwrap().entity_layers.clone()
    }

    pub fn get_mechanics_island_data(&self) -> Option<MechanicsIslandData> {
        let data = self.data.lock().unwrap();
        data.island_config
            .as_ref()
            .map(|config| MechanicsIslandData::new(config.clone(), data.rooms.clone()))
    }
}

impl UserData for Island {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_tile_layers", |_lua, this, layers: Table| {
            let mut layer_vec = Vec::new();
            for value in layers.sequence_values::<String>() {
                layer_vec.push(value?);
            }
            this.data.lock().unwrap().tile_layers = layer_vec;
            Ok(())
        });

        methods.add_method("set_entity_layers", |_lua, this, layers: Table| {
            let mut layer_vec = Vec::new();
            for value in layers.sequence_values::<String>() {
                layer_vec.push(value?);
            }
            this.data.lock().unwrap().entity_layers = layer_vec;
            Ok(())
        });

        methods.add_method("register_tile_field", |_lua, this, (tile_type, field_name, field_type, options): (String, String, String, Table)| {
            let field_options = parse_field_options(options)?;
            let registration = FieldRegistration {
                field_name,
                field_type,
                options: field_options,
            };

            let mut data = this.data.lock().unwrap();
            data.tile_fields.entry(tile_type).or_insert_with(Vec::new).push(registration);
            Ok(())
        });

        methods.add_method("register_entity_field", |_lua, this, (entity_type, field_name, field_type, options): (String, String, String, Table)| {
            let field_options = parse_field_options(options)?;
            let registration = FieldRegistration {
                field_name,
                field_type,
                options: field_options,
            };

            let mut data = this.data.lock().unwrap();
            data.entity_fields.entry(entity_type).or_insert_with(Vec::new).push(registration);
            Ok(())
        });

        methods.add_method("load_island_config", |_lua, this, path: String| {
            let mut data = this.data.lock().unwrap();
            let full_path = validate_path(Path::new(&path), &data.base_path)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let content = std::fs::read_to_string(&full_path).map_err(|e| {
                LuaError::RuntimeError(format!("Failed to read island config from {}: {}", path, e))
            })?;
            let island: MechanicsIsland = ron::from_str(&content).map_err(|e| {
                LuaError::RuntimeError(format!("Failed to parse island config: {}", e))
            })?;

            // Note: instead of doing full_path.parent() just use data.base_path around line 143.
            // (Note: The room loading loop that used ron_dir was removed as we now use explicit register_room calls)
            let _ron_dir = &data.base_path;

            data.island_config = Some(island);
            Ok(())
        });

        methods.add_method("load_entity_spawn", |_lua, this, path: String| {
            let mut data = this.data.lock().unwrap();
            let full_path = validate_path(Path::new(&path), &data.base_path)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let content = std::fs::read_to_string(&full_path).map_err(|e| {
                LuaError::RuntimeError(format!("Failed to read entity spawn from {}: {}", path, e))
            })?;
            let spawn: EntitySpawn = ron::from_str(&content).map_err(|e| {
                LuaError::RuntimeError(format!("Failed to parse entity spawn: {}", e))
            })?;
            data.entity_spawns.push(spawn);
            Ok(())
        });

        methods.add_method("register_process_fn", |lua, this, func: Function| {
            let mut data = this.data.lock().unwrap();
            let key = lua.create_registry_value(func)?;
            // TODO: if a room has functions set then it acts as a replacement and the global function isn't run.
            data.process_fn = Some(key);
            Ok(())
        });

        methods.add_method(
            "register_physics_process_fn",
            |lua, this, func: Function| {
                let mut data = this.data.lock().unwrap();
                let key = lua.create_registry_value(func)?;
                // TODO: if a room has functions set then it acts as a replacement and the global function isn't run.
                data.physics_process_fn = Some(key);
                Ok(())
            },
        );

        methods.add_method("register_room", |lua, this, (path, options): (String, Table)| {
            let mut data = this.data.lock().unwrap();
            let full_path = validate_path(Path::new(&path), &data.base_path)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            let room_content = std::fs::read_to_string(&full_path).map_err(|e| {
                LuaError::RuntimeError(format!("Failed to read room file {}: {}", path, e))
            })?;
            let room: Room = ron::from_str(&room_content).map_err(|e| {
                LuaError::RuntimeError(format!("Failed to parse room file {}: {}", path, e))
            })?;
            
            let room_id = room.room_id;
            data.rooms.push(room);

            if let Some(process_fn) = options.get::<Option<Function>>("process")? {
                data.room_process_fns.insert(room_id, lua.create_registry_value(process_fn)?);
            }
            if let Some(physics_process_fn) = options.get::<Option<Function>>("physics_process")? {
                data.room_physics_process_fns.insert(room_id, lua.create_registry_value(physics_process_fn)?);
            }
            Ok(())
        });

        methods.add_method(
            "register_gltf",
            |_lua, this, (name, path): (String, String)| {
                validate_filename(&name)
                    .map_err(|e| LuaError::RuntimeError(format!("Invalid GLTF name: {}", e)))?;
                let mut data = this.data.lock().unwrap();
                let fullpath = validate_path(Path::new(&path), &data.base_path).unwrap();
                data.gltf_registry.insert(name, fullpath);
                Ok(())
            },
        );

        methods.add_method("get_room_count", |_lua, this, ()| {
            let data = this.data.lock().unwrap();
            Ok(data.rooms.len())
        });

        methods.add_method("get_entity_spawn_count", |_lua, this, ()| {
            let data = this.data.lock().unwrap();
            Ok(data.entity_spawns.len())
        });

        methods.add_method(
            "rooms_are_adjacent",
            |_lua, this, (room_a_id, room_b_id): (u32, u32)| {
                let mechanics_data = this.get_mechanics_island_data().ok_or_else(|| {
                    LuaError::RuntimeError("Island config not loaded".to_string())
                })?;
                Ok(mechanics_data.rooms_are_adjacent(room_a_id, room_b_id))
            },
        );
    }
}

fn parse_field_options(options: Table) -> mlua::Result<FieldOptions> {
    let default = options
        .get::<Option<Value>>("default")?
        .and_then(|v| match v {
            Value::Integer(i) => Some(DefaultValue::Int(i)),
            Value::Number(n) => Some(DefaultValue::Float(n)),
            Value::String(s) => s.to_str().ok().map(|s| DefaultValue::String(s.to_string())),
            Value::Boolean(b) => Some(DefaultValue::Bool(b)),
            _ => None,
        });

    let min = options.get::<Option<i64>>("min")?;
    let max = options.get::<Option<i64>>("max")?;

    let (values, value_type) = match options.get::<Option<Value>>("values")? {
        Some(Value::Table(t)) => {
            let mut vec = Vec::new();
            for value in t.sequence_values::<String>() {
                if let Ok(v) = value {
                    vec.push(v);
                }
            }
            (Some(vec), None)
        }
        Some(Value::String(s)) => (None, s.to_str().ok().map(|s| s.to_string())),
        _ => (None, None),
    };

    let keys = options.get::<Option<String>>("keys")?;
    let item_type = options.get::<Option<String>>("item_type")?;

    let schema = options.get::<Option<Table>>("schema")?.map(|t| {
        let mut map = HashMap::new();
        for pair in t.pairs::<String, String>() {
            if let Ok((k, v)) = pair {
                map.insert(k, v);
            }
        }
        map
    });

    Ok(FieldOptions {
        default,
        min,
        max,
        values,
        keys,
        value_type,
        item_type,
        schema,
    })
}

pub fn create_lua_sandbox_and_island() -> (Lua, Island) {
    let lua = Lua::new();
    lua.sandbox(true).expect("failed to create sandbox");

    let island = Island::new();
    lua.globals()
        .set("island", island.clone())
        .expect("failed to set island global");

    (lua, island)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_set_tile_layers() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            local layers = {"Background", "Floor", "Walls"}
            island:set_tile_layers(layers)
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");

        // Assert
        assert_eq!(
            island.get_tile_layers(),
            vec!["Background", "Floor", "Walls"]
        );
    }

    #[test]
    fn test_set_entity_layers() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            local layers = {"Actors", "Triggers", "Items"}
            island:set_entity_layers(layers)
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");

        // Assert
        assert_eq!(
            island.get_entity_layers(),
            vec!["Actors", "Triggers", "Items"]
        );
    }

    #[test]
    fn test_register_tile_field_with_int() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            island:register_tile_field("lava_tile", "damage_on_touch", "int", { default = 10 })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        let fields = data
            .tile_fields
            .get("lava_tile")
            .expect("lava_tile not found");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "damage_on_touch");
        assert_eq!(fields[0].field_type, "int");
        assert!(fields[0].options.default.is_some());
    }

    #[test]
    fn test_register_tile_field_with_enum() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            local DamageType = {"Physical", "Fire", "Cold"}
            island:register_tile_field("lava_tile", "damage_type", "enum", { values = DamageType, default = "Fire" })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        let fields = data
            .tile_fields
            .get("lava_tile")
            .expect("lava_tile not found");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "damage_type");
        assert_eq!(fields[0].field_type, "enum");
        let values = fields[0].options.values.as_ref().expect("values not found");
        assert_eq!(values, &vec!["Physical", "Fire", "Cold"]);
    }

    #[test]
    fn test_register_tile_field_with_map() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            island:register_tile_field("teleport_tile", "destination", "map", { keys = "string", values = "int" })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        let fields = data
            .tile_fields
            .get("teleport_tile")
            .expect("teleport_tile not found");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "destination");
        assert_eq!(fields[0].field_type, "map");
        assert_eq!(fields[0].options.keys, Some("string".to_string()));
    }

    #[test]
    fn test_register_tile_field_with_list() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            island:register_tile_field("sign_tile", "messages", "list", { item_type = "string" })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        let fields = data
            .tile_fields
            .get("sign_tile")
            .expect("sign_tile not found");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "messages");
        assert_eq!(fields[0].field_type, "list");
        assert_eq!(fields[0].options.item_type, Some("string".to_string()));
    }

    #[test]
    fn test_register_entity_field_with_int_range() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            island:register_entity_field("npc_basic", "health", "int", { min = 1, max = 1000, default = 100 })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        let fields = data
            .entity_fields
            .get("npc_basic")
            .expect("npc_basic not found");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "health");
        assert_eq!(fields[0].field_type, "int");
        assert_eq!(fields[0].options.min, Some(1));
        assert_eq!(fields[0].options.max, Some(1000));
    }

    #[test]
    fn test_register_entity_field_with_map_schema() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            island:register_entity_field("npc_basic", "stats", "map", { keys = "string", values = "int" })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        let fields = data
            .entity_fields
            .get("npc_basic")
            .expect("npc_basic not found");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_name, "stats");
        assert_eq!(fields[0].field_type, "map");
        assert_eq!(fields[0].options.keys, Some("string".to_string()));
    }

    #[test]
    fn test_multiple_fields_on_same_entity() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            local AIBehavior = {"Idle", "Patrol", "Aggressive"}
            island:register_entity_field("npc_basic", "health", "int", { default = 100 })
            island:register_entity_field("npc_basic", "behavior", "enum", { values = AIBehavior, default = "Idle" })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        let fields = data
            .entity_fields
            .get("npc_basic")
            .expect("npc_basic not found");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_name, "health");
        assert_eq!(fields[1].field_name, "behavior");
    }

    #[test]
    fn test_load_island_config() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let ron_dir = temp_dir.path().join("ron");
        fs::create_dir(&ron_dir).unwrap();

        let (lua, island) = create_lua_sandbox_and_island();
        island.data.lock().unwrap().base_path = temp_dir.path().to_path_buf();

        // Create island config file
        let island_ron = r#"(
            dock_room_id: 1,
            name: "Test Island",
            description: "A test island for loading",
        )"#;

        fs::write(ron_dir.join("island.ron"), island_ron).unwrap();

        // Create room_1.ron
        let room_ron = r#"(
            room_id: 1,
            pos_x: 0, pos_y: 0, pos_z: 0,
            extent_x: 5, extent_y: 5, extent_z: 5,
            looping_x: false, looping_y: false, looping_z: false,
            tiles: {},
        )"#;
        fs::write(ron_dir.join("room_1.ron"), room_ron).unwrap();

        let script = r#"
            island:load_island_config("ron/island.ron")
            island:register_room("ron/room_1.ron", {})
        "#;
        lua.load(script).exec().expect("Failed to execute script");

        let data = island.data.lock().unwrap();
        assert!(data.island_config.is_some());
        assert_eq!(data.island_config.as_ref().unwrap().name, "Test Island");
        assert_eq!(data.rooms.len(), 1, "Should have loaded registered room");
    }

    #[test]
    fn test_load_island_config_manual_registration() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let ron_dir = temp_dir.path().join("ron");
        fs::create_dir(&ron_dir).unwrap();

        let (lua, island) = create_lua_sandbox_and_island();
        island.data.lock().unwrap().base_path = temp_dir.path().to_path_buf();

        // Create island config file
        let island_ron = r#"(
            dock_room_id: 1,
            name: "Test Island",
            description: "A test island for loading",
        )"#;
        fs::write(ron_dir.join("island.ron"), island_ron).unwrap();

        // Create room_1.ron (dock room)
        let room1_ron = r#"(
            room_id: 1,
            pos_x: 0, pos_y: 0, pos_z: 0,
            extent_x: 5, extent_y: 5, extent_z: 5,
            looping_x: false, looping_y: false, looping_z: false,
            tiles: {},
        )"#;
        fs::write(ron_dir.join("room_1.ron"), room1_ron).unwrap();

        // Create room_2.ron (disconnected room)
        let room2_ron = r#"(
            room_id: 2,
            pos_x: 10, pos_y: 0, pos_z: 0,
            extent_x: 5, extent_y: 5, extent_z: 5,
            looping_x: false, looping_y: false, looping_z: false,
            tiles: {},
        )"#;
        fs::write(ron_dir.join("room_2.ron"), room2_ron).unwrap();

        let script = r#"
            island:load_island_config("ron/island.ron")
            island:register_room("ron/room_1.ron", {})
            island:register_room("ron/room_2.ron", {})
        "#;
        lua.load(script).exec().expect("Failed to execute script");

        let data = island.data.lock().unwrap();
        assert_eq!(data.rooms.len(), 2, "Both rooms should be loaded via explicit registration");
    }

    #[test]
    fn test_load_entity_spawn() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let ron_dir = temp_dir.path().join("ron").join("spawns");
        fs::create_dir_all(&ron_dir).unwrap();

        let (lua, island) = create_lua_sandbox_and_island();
        island.data.lock().unwrap().base_path = temp_dir.path().to_path_buf();

        let spawn_ron = r#"(
            entity_type: "npc_basic",
            room_id: 1,
            grid_index: 5,
            properties: {
                "health": "100",
            },
        )"#;

        fs::write(ron_dir.join("enemy_1.ron"), spawn_ron).unwrap();

        let script = r#"
            island:load_entity_spawn("ron/spawns/enemy_1.ron")
            assert(island:get_entity_spawn_count() == 1, "Entity spawn count should be 1")
        "#;

        lua.load(script).exec().expect("Failed to execute script");

        let data = island.data.lock().unwrap();
        assert_eq!(data.entity_spawns.len(), 1);
        assert_eq!(data.entity_spawns[0].entity_type, "npc_basic");
    }

    #[test]
    fn test_register_gltf() {
        use std::fs;
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("models")).unwrap();

        let (lua, island) = create_lua_sandbox_and_island();
        island.data.lock().unwrap().base_path = temp_dir.path().to_path_buf();

        let script = r#"
            island:register_gltf("character", "models/character.gltf")
            island:register_gltf("tree", "models/tree.gltf")
        "#;

        lua.load(script).exec().expect("Failed to execute script");

        let data = island.data.lock().unwrap();
        assert_eq!(data.gltf_registry.len(), 2);
        assert!(data.gltf_registry.contains_key("character"));
        assert!(data.gltf_registry.contains_key("tree"));
    }

    #[test]
    fn test_rooms_are_adjacent_from_luau() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let ron_dir = temp_dir.path().join("ron");
        fs::create_dir(&ron_dir).unwrap();

        let (lua, island) = create_lua_sandbox_and_island();
        island.data.lock().unwrap().base_path = temp_dir.path().to_path_buf();

        // Create island config
        let island_ron = r#"(
            dock_room_id: 1,
            name: "Test",
            description: "Test",
        )"#;
        fs::write(ron_dir.join("island.ron"), island_ron).unwrap();

        // Create two adjacent rooms with door connection
        let room1_ron = r#"(
            room_id: 1,
            pos_x: 0, pos_y: 0, pos_z: 0,
            extent_x: 5, extent_y: 5, extent_z: 5,
            looping_x: false, looping_y: false, looping_z: false,
            tiles: {
                10: Door(1, 2),
            },
        )"#;

        let room2_ron = r#"(
            room_id: 2,
            pos_x: 5, pos_y: 0, pos_z: 0,
            extent_x: 5, extent_y: 5, extent_z: 5,
            looping_x: false, looping_y: false, looping_z: false,
            tiles: {},
        )"#;

        fs::write(ron_dir.join("room_1.ron"), room1_ron).unwrap();
        fs::write(ron_dir.join("room_2.ron"), room2_ron).unwrap();

        let script = r#"
            island:load_island_config("ron/island.ron")
            island:register_room("ron/room_1.ron", {})
            island:register_room("ron/room_2.ron", {})

            local adjacent = island:rooms_are_adjacent(1, 2)
            assert(adjacent == true, "Rooms should be adjacent")

            local not_adjacent = island:rooms_are_adjacent(1, 999)
            assert(not_adjacent == false, "Non-existent room should not be adjacent")
        "#;

        lua.load(script).exec().expect("Failed to execute script");
    }

    #[test]
    fn test_full_campaign_script() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        let script = r#"
            local TileLayers = {"Background", "Floor", "Walls", "Decoration", "Overlay"}
            local EntityLayers = {"Actors", "Triggers", "Items", "VFX"}

            island:set_tile_layers(TileLayers)
            island:set_entity_layers(EntityLayers)

            local DamageType = {"Physical", "Fire", "Cold", "Lightning", "Void"}
            local AIBehavior = {"Idle", "Patrol", "Aggressive", "Flee"}

            island:register_tile_field("lava_tile", "damage_on_touch", "int", { default = 10 })
            island:register_tile_field("lava_tile", "damage_type", "enum", { values = DamageType, default = "Fire" })
            island:register_tile_field("teleport_tile", "destination", "map", { keys = "string", values = "int" })
            island:register_tile_field("sign_tile", "messages", "list", { item_type = "string" })

            island:register_entity_field("npc_basic", "health", "int", { min = 1, max = 1000, default = 100 })
            island:register_entity_field("npc_basic", "behavior", "enum", { values = AIBehavior, default = "Idle" })
            island:register_entity_field("npc_basic", "inventory_tags", "list", { item_type = "string" })
            island:register_entity_field("npc_basic", "stats", "map", { keys = "string", values = "int" })
        "#;

        // Act
        lua.load(script).exec().expect("failed to execute script");
        let data = island.data.lock().unwrap();

        // Assert
        assert_eq!(data.tile_layers.len(), 5);
        assert_eq!(data.entity_layers.len(), 4);
        assert_eq!(data.tile_fields.get("lava_tile").unwrap().len(), 2);
        assert_eq!(data.tile_fields.get("teleport_tile").unwrap().len(), 1);
        assert_eq!(data.tile_fields.get("sign_tile").unwrap().len(), 1);
        assert_eq!(data.entity_fields.get("npc_basic").unwrap().len(), 4);
    }

    #[test]
    fn test_load_tbol_vanilla() {
        // Arrange
        let (lua, island) = create_lua_sandbox_and_island();
        
        // island:new() sets base_path to "tbol_vanilla" by default.
        // If we're in the workspace root, this is correct.
        // However, if we're in tbol_gdext, we need to go up one level.
        // Let's check where we are and adjust base_path if needed.
        if !std::path::Path::new("tbol_vanilla").exists() && std::path::Path::new("../tbol_vanilla").exists() {
             island.data.lock().unwrap().base_path = std::path::PathBuf::from("../tbol_vanilla");
        }

        let base_path = island.data.lock().unwrap().base_path.clone();
        let script_path = base_path.join("island.luau");
        let script = std::fs::read_to_string(&script_path)
            .unwrap_or_else(|e| panic!("Failed to read island.luau from {:?}: {}", script_path, e));

        // Act
        lua.load(&script).exec().expect("Failed to execute vanilla island.luau");

        // Assert
        let data = island.data.lock().unwrap();
        
        // Validation check for top-level loading
        assert!(!data.tile_layers.is_empty(), "Tile layers should be loaded");
        assert!(!data.entity_layers.is_empty(), "Entity layers should be loaded");
        
        assert!(data.tile_fields.contains_key("lava_tile"), "Lava tile fields should be registered");
        assert!(data.entity_fields.contains_key("npc_basic"), "NPC basic fields should be registered");
        assert!(data.gltf_registry.contains_key("character"), "Character GLTF should be registered");
        
        assert!(data.island_config.is_some(), "Island config should be loaded");
        assert!(!data.rooms.is_empty(), "Rooms should be loaded");
        assert!(!data.entity_spawns.is_empty(), "Entity spawns should be loaded");
        
        println!("Successfully validated vanilla island loading.");
    }
}
