# Level JSON Format

This editor saves levels in a simple JSON format that describes entity positions and types. Colliders and physics properties are configured in your game code based on entity type.

## Example Level JSON

```json
{
  "version": "1.0",
  "background": null,
  "background_size": [1920.0, 1080.0],
  "entities": [
    {
      "type": "bitmap",
      "position": [100.0, 200.0],
      "bitmap_name": "platform",
      "size": [128.0, 32.0]
    },
    {
      "type": "bitmap",
      "position": [300.0, 150.0],
      "bitmap_name": "coin",
      "size": [32.0, 32.0]
    },
    {
      "type": "polygon",
      "vertices": [
        [50.0, 100.0],
        [150.0, 100.0],
        [150.0, 150.0],
        [50.0, 150.0]
      ]
    }
  ]
}
```

## Schema

### LevelData

Root object for a level file.

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Format version (currently "1.0") |
| `background` | string? | Optional path to background image |
| `background_size` | [f32; 2]? | Optional background dimensions [width, height] |
| `entities` | LevelEntity[] | Array of entities in the level |

### LevelEntity

Tagged union representing different entity types.

#### Bitmap Entity

```json
{
  "type": "bitmap",
  "position": [x, y],
  "bitmap_name": "asset_name",
  "size": [width, height]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `position` | [f32; 2] | World position [x, y] |
| `bitmap_name` | string | Asset name/path for the sprite (e.g., "platform", "coin", "enemy") |
| `size` | [f32; 2] | Dimensions [width, height] |

#### Rope (a bitmap entry)

A rope is saved as a bitmap entry named `rope_tool`, so the schema is unchanged and
consumers that do not know ropes see an ordinary bitmap:

```json
{
  "type": "bitmap",
  "position": [400.0, 100.0],
  "bitmap_name": "rope_tool",
  "size": [6.0, 300.0]
}
```

| Field | Meaning for a rope |
|-------|--------------------|
| `position` | The top anchor the rope hangs from |
| `size[0]` | Rope thickness (always 6) |
| `size[1]` | Rope length in world pixels, at least 60 |

The rope hangs straight down from the anchor (toward +y in the editor's top-left
coordinates). Swing behaviour is game code; the editor's play mode is the reference.

#### Polygon Entity

```json
{
  "type": "polygon",
  "vertices": [[x1, y1], [x2, y2], ...]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `vertices` | [[f32; 2]] | Array of vertex positions |

## Using in Bevy + Avian

```rust
use bevy::prelude::*;
use avian2d::prelude::*;
use serde_json;

// Load level JSON
fn load_level(
    level_json: &str,
    commands: &mut Commands,
    asset_server: &AssetServer,
) {
    let level: LevelData = serde_json::from_str(level_json).unwrap();
    
    for entity in level.entities {
        match entity {
            LevelEntity::Bitmap { position, bitmap_name, size } => {
                let mut entity_builder = commands.spawn((
                    SpriteBundle {
                        texture: asset_server.load(&format!("sprites/{}.png", bitmap_name)),
                        transform: Transform::from_xyz(position[0], position[1], 0.0),
                        sprite: Sprite {
                            custom_size: Some(Vec2::new(size[0], size[1])),
                            ..default()
                        },
                        ..default()
                    },
                    RigidBody::Static,
                ));
                
                // Add colliders based on entity type
                match bitmap_name.as_str() {
                    "platform" | "ground" => {
                        entity_builder.insert((
                            Collider::rectangle(size[0], size[1]),
                            Friction::new(0.5),
                        ));
                    }
                    "coin" | "collectible" => {
                        entity_builder.insert((
                            Collider::circle(size[0] / 2.0),
                            Sensor,  // Pass-through trigger
                        ));
                    }
                    "enemy" => {
                        entity_builder.insert((
                            Collider::circle(size[0] / 2.0),
                            RigidBody::Dynamic,
                        ));
                    }
                    _ => {}
                }
            }
            LevelEntity::Polygon { vertices } => {
                let vecs: Vec<Vec2> = vertices.iter()
                    .map(|v| Vec2::new(v[0], v[1]))
                    .collect();
                commands.spawn((
                    SpatialBundle::default(),
                    RigidBody::Static,
                    Collider::convex_hull(vecs).unwrap(),
                    Friction::new(0.5),
                ));
            }
        }
    }
}
```

## Tips

1. **Configure colliders in game code** based on `bitmap_name` for flexibility
2. **Test colliders**: Use Avian's debug renderer to visualize
   ```rust
   .add_plugins(PhysicsDebugPlugin::default())
   ```
3. **Sensors for collectibles**: Mark coins/power-ups as sensors (pass-through triggers)
4. **Polygon platforms**: Automatically get convex hull colliders
5. **Layer objects**: Use the Z coordinate for sprite layering
