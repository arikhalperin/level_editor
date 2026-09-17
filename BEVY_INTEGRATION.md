# Bevy Integration Guide

Quick guide for integrating the level editor JSON files into your Bevy + Avian2D game.

## Philosophy

The JSON format is intentionally simple - it only describes **what** entities exist and **where** they are. Your game code decides **how** they behave (colliders, physics, components). This gives you flexibility to change gameplay without re-editing levels.

## Dependencies

Add these to your Bevy project's `Cargo.toml`:

```toml
[dependencies]
bevy = "0.14"  # or latest version
avian2d = "0.1"  # or latest version
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

## Copy the Level Data Module

Copy `src/level_data.rs` from the editor project to your game project. This provides the data structures for deserializing level JSON files.

## Ropes

A rope is a bitmap entry whose `bitmap_name` is `rope_tool`: `position` is the anchor and
`size` is `[thickness, length]`. Spawn a rope entity from it rather than a sprite. The
swing model (grab by holding up/down, pumped pendulum capped at 75°, climb along at the
climb speed, release with the swing velocity plus a wall-jump-height hop) and its named
constants live in the editor's `src/game_config.rs` under `ROPE_*`.

## Loading Levels

### Basic Level Loader System

```rust
use bevy::prelude::*;
use avian2d::prelude::*;
use serde_json;

// Include the level_data module
mod level_data;
use level_data::{LevelData, LevelEntity, ColliderType};

#[derive(Resource)]
struct CurrentLevel(Handle<TextAsset>);

fn load_level_json(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    // Load level JSON as an asset
    let level_handle = asset_server.load("levels/level1.json");
    commands.insert_resource(CurrentLevel(level_handle));
}

fn spawn_level_entities(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut level_assets: ResMut<Assets<TextAsset>>,
    current_level: Res<CurrentLevel>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }

    if let Some(level_text) = level_assets.get(&current_level.0) {
        let level_data: LevelData = serde_json::from_str(&level_text.text).unwrap();
        
        for entity in level_data.entities {
            match entity {
                LevelEntity::Bitmap { position, bitmap_name, size } => {
                    spawn_bitmap_entity(
                        &mut commands,
                        &asset_server,
                        position,
                        bitmap_name,
                        size,
                    );
                }
                LevelEntity::Polygon { vertices } => {
                    spawn_polygon_entity(&mut commands, vertices);
                }
            }
        }
        
        *done = true;
    }
}

fn spawn_bitmap_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    position: [f32; 2],
    bitmap_name: String,
    size: [f32; 2],
) {
    let mut entity_builder = commands.spawn((
        SpriteBundle {
            texture: asset_server.load(format!("sprites/{}.png", bitmap_name)),
            transform: Transform::from_xyz(position[0], position[1], 0.0),
            sprite: Sprite {
                custom_size: Some(Vec2::new(size[0], size[1])),
                ..default()
            },
            ..default()
        },
        RigidBody::Static,
    ));
    
    // Configure colliders and physics based on entity type
    match bitmap_name.as_str() {
        "platform" | "ground" | "wall" => {
            entity_builder.insert((
                Collider::rectangle(size[0], size[1]),
                Friction::new(0.5),
            ));
        }
        "coin" | "gem" | "collectible" => {
            entity_builder.insert((
                Collider::circle(size[0] / 2.0),
                Sensor,  // Pass-through trigger
            ));
        }
        "enemy" | "ork" => {
            entity_builder.insert((
                Collider::circle(size[0] / 2.0),
                RigidBody::Dynamic,
                Friction::new(0.3),
            ));
        }
        "spike" | "danger" => {
            entity_builder.insert((
                Collider::rectangle(size[0], size[1]),
                Sensor,  // Damage trigger
            ));
        }
        _ => {}
    }
}

fn spawn_polygon_entity(
    commands: &mut Commands,
    vertices: Vec<[f32; 2]>,
) {
    let vecs: Vec<Vec2> = vertices
        .iter()
        .map(|v| Vec2::new(v[0], v[1]))
        .collect();
    
    commands.spawn((
        SpatialBundle::default(),
        RigidBody::Static,
        Collider::convex_hull(vecs).unwrap(),
        Friction::new(0.5),
    ));
}

fn add_collider_components(
    entity: &mut EntityCommands,
    collider_data: ColliderData,
) {
    // Add the appropriate collider based on type
    match collider_data.collider_type {
        ColliderType::Box { half_extents } => {
            entity.insert(Collider::rectangle(
                half_extents[0] * 2.0,
                half_extents[1] * 2.0,
            ));
        }
        ColliderType::ConvexHull { points } => {
            let vecs: Vec<Vec2> = points
                .iter()
                .map(|p| Vec2::new(p[0], p[1]))
                .collect();
            if let Some(collider) = Collider::convex_hull(vecs) {
                entity.insert(collider);
            }
        }
        ColliderType::Polyline { points } => {
            let vecs: Vec<Vec2> = points
                .iter()
                .map(|p| Vec2::new(p[0], p[1]))
                .collect();
            entity.insert(Collider::polyline(vecs, None));
        }
    }
    
    // Add physics properties
    entity.insert((
        Friction::new(collider_data.friction),
        Restitution::new(collider_data.restitution),
    ));
    
    // Add sensor flag if needed
    if collider_data.is_sensor {
        entity.insert(Sensor);
    }
}

// Add systems to your app
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(PhysicsPlugins::default())
        .add_systems(Startup, load_level_json)
        .add_systems(Update, spawn_level_entities)
        .run();
}
```

## Coordinate System Conversion

The editor uses egui's coordinate system (top-left origin, Y-down). Bevy uses bottom-left origin with Y-up. You may need to flip Y coordinates:

```rust
fn convert_coords(editor_pos: [f32; 2], background_height: f32) -> Vec2 {
    Vec2::new(
        editor_pos[0],
        background_height - editor_pos[1]  // Flip Y
    )
}
```

## Component Markers

Add custom components to identify entity types:

```rust
#[derive(Component)]
struct Platform;

#[derive(Component)]
struct Collectible;

#[derive(Component)]
struct Enemy;

// When spawning:
match bitmap_name.as_str() {
    "platform" => entity.insert(Platform),
    "coin" => entity.insert(Collectible),
    "ork" => entity.insert(Enemy),
    _ => entity,
};
```

## Collision Groups

Use collision groups for filtering what can collide:

```rust
use avian2d::prelude::*;

const PLAYER_GROUP: u32 = 0b0001;
const PLATFORM_GROUP: u32 = 0b0010;
const ENEMY_GROUP: u32 = 0b0100;
const COLLECTIBLE_GROUP: u32 = 0b1000;

entity.insert(CollisionLayers::new(
    PLATFORM_GROUP,  // This entity is in platform group
    PLAYER_GROUP | ENEMY_GROUP,  // Can collide with player and enemies
));
```

## Example: Loading from File System

For quick iteration during development:

```rust
use std::fs;

fn load_level_from_file(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    let json = fs::read_to_string("assets/levels/level1.json")
        .expect("Failed to read level file");
    
    let level_data: LevelData = serde_json::from_str(&json)
        .expect("Failed to parse level JSON");
    
    // Spawn entities...
}
```

## Tips

1. **Configure colliders in code** - Change gameplay without re-editing levels
2. **Use bitmap_name for logic** - Match on entity names to add appropriate components/colliders
3. **Test colliders**: Use Avian's debug renderer to visualize colliders
   ```rust
   .add_plugins(PhysicsDebugPlugin::default())
   ```

4. **One-way platforms**: Use polyline colliders with normal direction
   ```rust
   entity.insert(Collider::polyline(points, Some(Vec2::Y)));
   ```

5. **Static vs Dynamic**: Platforms should be `RigidBody::Static`, moving platforms `RigidBody::Kinematic`

6. **Layer objects**: Use the Z coordinate for layering sprites
   ```rust
   Transform::from_xyz(position[0], position[1], layer_z)
   ```

7. **Scale sprites**: If your game uses a different scale, multiply positions
   ```rust
   let game_scale = 2.0;
   Vec2::new(position[0] * game_scale, position[1] * game_scale)
   ```

8. **Separate concerns**: Let the editor place entities, let the game code define behavior
