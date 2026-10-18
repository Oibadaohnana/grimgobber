//! How far the robber's noise carries, and showing it for a smashed window
//! or a coin landing: a faint ring sweeps out to as far as the wraith hears it
//! (`Sight::carry`, round corners and weakly through walls). Drawn tile by
//! tile into a texture the shader draws the ring from.

use crate::doors::Shut;
use crate::movement::Body;
use crate::render::WorldMaterials;
use crate::sight::{Sight, smoothstep};
use crate::sound::{Clip, Noise};
use bevy::prelude::*;

/// How far off the robber's feet are heard, in tiles: walking,
/// sprinting, rolling (crouched, nothing).
pub const WALK_LOUD: f32 = 2.5;
pub const SPRINT_LOUD: f32 = 6.0;
pub const ROLL_LOUD: f32 = 3.0;
/// The ring sweeps out to its reach in this long, then fades between these ages.
const SPREAD: f32 = 0.4;
const FADE: (f32, f32) = (0.6, 1.3);
/// Tiles of reach left, as a texel: a sixteenth of a tile a step.
const STEPS: f32 = 16.0;

pub fn feet(b: &Body) -> f32 {
    if b.is_rolling() {
        ROLL_LOUD
    } else if b.speed < 1.0 || b.is_crouching() {
        0.0
    } else if b.is_sprinting() {
        SPRINT_LOUD
    } else {
        WALK_LOUD
    }
}

/// Noises still showing: where, how many tiles they carry, how old.
#[derive(Resource, Default)]
struct Ripples(Vec<(Vec2, f32, f32)>);

pub struct HearingPlugin;

impl Plugin for HearingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ripples>().add_systems(
            Update,
            ripple.run_if(resource_exists::<Shut>.and_then(resource_exists::<Sight>)),
        );
    }
}

fn ripple(
    time: Res<Time>,
    sight: Res<Sight>,
    world_materials: Res<WorldMaterials>,
    shut: Res<Shut>,
    mut images: ResMut<Assets<Image>>,
    mut noises: MessageReader<Noise>,
    mut ripples: ResMut<Ripples>,
) {
    let dt = time.delta_secs();
    for r in &mut ripples.0 {
        r.2 += dt;
    }
    ripples.0.retain(|r| r.2 < FADE.1);
    ripples.0.extend(
        noises
            .read()
            .filter(|n| matches!(n.clip, Some(Clip::Glass | Clip::Coin)))
            .map(|n| (n.at, n.loud, 0.0)),
    );
    let doors = &shut.0;
    let shown = ripples.0.iter().map(|&(at, loud, age)| {
        let strength = 1.0 - smoothstep(FADE.0, FADE.1, age);
        (at, loud * (age / SPREAD).min(1.0), strength)
    });
    // Per tile: the tiles of reach left there (just reached is 1, so it is
    // not mistaken for silence), and how strongly to show it.
    let Some(mut image) = images.get_mut(&world_materials.heard) else {
        return;
    };
    let Some(data) = image.data.as_mut() else {
        return;
    };
    data.fill(0);
    for (at, reach, strength) in shown {
        if reach <= 0.0 {
            continue;
        }
        for (i, left) in sight.carry(at, reach, doors).into_iter().enumerate() {
            if left > 0.0 && i * 4 + 1 < data.len() {
                let left = ((left * STEPS).min(255.0) as u8).max(1);
                data[i * 4] = data[i * 4].max(left);
                data[i * 4 + 1] = data[i * 4 + 1].max((strength * 255.0) as u8);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{generate, tile_centre};
    use crate::movement::{Aabb, STEP, Solids, Steer, TILE};
    use crate::sight::Sight;

    #[test]
    fn crouched_feet_make_no_sound() {
        let solids = Solids::new(Aabb::new(Vec2::ZERO, Vec2::splat(20.0 * TILE)), Vec::new());
        let walked = |crouch| {
            let mut b = Body::new(Vec2::splat(10.0 * TILE), 0.0);
            b.set_steer(Steer {
                walk: Some(0.0),
                crouch,
                ..default()
            });
            for _ in 0..30 {
                b.update(STEP, &solids);
            }
            feet(&b)
        };
        assert_eq!(walked(false), WALK_LOUD);
        assert_eq!(walked(true), 0.0);
    }

    #[test]
    fn noises_go_round_corners_and_lose_reach_to_walls() {
        let sight = Sight::new(&generate(6));
        let hears = |ear: (i32, i32), at: (i32, i32), loud: f32| {
            let ear = sight.index(tile_centre(ear.0, ear.1)).unwrap();
            sight.carry(tile_centre(at.0, at.1), loud, &[])[ear] > 0.0
        };
        // Along the pavement, 4 tiles: a sprint is heard, a walk is not.
        assert!(hears((30, 8), (34, 8), SPRINT_LOUD));
        assert!(!hears((30, 8), (34, 8), WALK_LOUD));
        // 4 tiles through the bank's brick front cost 4 more (the doors
        // are 8 tiles off, too far to go round by).
        assert!(!hears((21, 8), (21, 12), 7.5));
        assert!(hears((21, 8), (21, 12), 8.5));
    }
}
