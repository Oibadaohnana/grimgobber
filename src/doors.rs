//! Doors swing open, away from whoever comes: pushed by hand within reach
//! (the robber puts his arm out, and back as it shuts behind him), the glass
//! entrance by its sensor. How shut each is goes to the ray tracer, so a shut door stops
//! sight and lamplight. The leaf is drawn as the robber last saw it: in the
//! fog, nobody is seen opening doors.

use crate::guard::Guard;
use crate::map::{Door, DoorKind, Map, tile_centre};
use crate::movement::{BODY_MARGIN, TILE};
use crate::paint;
use crate::player::{Controls, Player, Reach};
use crate::render::{WorldMaterial, WorldMaterials, WorldPart};
use crate::sight::Sight;
use crate::sound::{Clip, Noise};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

/// A hand-opened door: someone this close to the doorway's line, in front of it.
const REACH: f32 = BODY_MARGIN + 0.5 * TILE;
/// The entrance's sensor: this close to the doorway's middle.
const SENSOR: f32 = 1.6 * TILE;
const OPENING: f32 = 0.15;
const CLOSING: f32 = 0.4;
/// How far off a guard hears the robber at a door, in tiles (crouched, not at all).
const LOUD: f32 = 4.0;

#[derive(Component)]
pub struct Leaf {
    door: Door,
    tile: usize,
    open: f32,
    /// How open it is drawn.
    shown: f32,
    /// Which way across the wall it swings, picked as it starts to open.
    side: f32,
    /// Opened by the robber, whose arm goes out to it.
    robber: bool,
}

pub struct DoorsPlugin;

impl Plugin for DoorsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            swing.run_if(resource_exists::<Player>.and_then(resource_exists::<Sight>)),
        );
    }
}

pub fn spawn_doors(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<WorldMaterial>,
    map: &Map,
) {
    for door in map.doors() {
        commands.spawn((
            WorldPart,
            Leaf {
                door,
                tile: map.idx(door.x, door.y),
                open: 0.0,
                shown: 0.0,
                side: 1.0,
                robber: false,
            },
            Mesh2d(meshes.add(paint::door_leaf(door.kind))),
            MeshMaterial2d(material.clone()),
            Transform::from_translation(door.hinge.extend(5.0))
                .with_rotation(Quat::from_rotation_z(door.along.to_angle())),
        ));
    }
}

/// The doorway on `eye`'s side of the leaf, so the door's own slab is not in
/// the way of seeing (or hearing) it.
fn near_side(d: Door, eye: Vec2) -> Vec2 {
    let centre = tile_centre(d.x, d.y);
    centre + d.across * (eye - centre).dot(d.across).signum() * 0.3 * TILE
}

#[allow(clippy::too_many_arguments)]
fn swing(
    time: Res<Time>,
    player: Res<Player>,
    controls: Res<Controls>,
    sight: Res<Sight>,
    guards: Query<&Guard>,
    mut reach: ResMut<Reach>,
    world_materials: Res<WorldMaterials>,
    mut images: ResMut<Assets<Image>>,
    mut leaves: Query<(&mut Leaf, &mut Transform)>,
    mut noises: MessageWriter<Noise>,
) {
    let walkers: Vec<_> = std::iter::once(&player.body)
        .chain(guards.iter().map(|g| &g.body))
        .map(|b| (b.pos, b.speed))
        .collect();
    let dt = time.delta_secs();
    let mut moved = Vec::new();
    for (mut leaf, _) in &mut leaves {
        let d = leaf.door;
        let centre = tile_centre(d.x, d.y);
        let by_hand = d.kind != DoorKind::Glass;
        let opener = walkers.iter().enumerate().find_map(|(i, &(pos, speed))| {
            let off = pos - centre;
            let gap = off.dot(d.across).abs();
            let near = if by_hand {
                gap < REACH && off.dot(d.along).abs() < TILE * 0.5 + BODY_MARGIN
            } else {
                off.length() < SENSOR
            };
            near.then_some((i == 0, off, gap, speed))
        });
        let near = opener.is_some();
        let was = leaf.open;
        if let Some((robber, off, _, _)) = opener
            && was == 0.0
        {
            leaf.side = if off.dot(d.across) > 0.0 { -1.0 } else { 1.0 };
            leaf.robber = robber;
        }
        let (opens, shuts) = (near && was == 0.0, !near && was == 1.0);
        if by_hand && leaf.robber && (opens || shuts) {
            reach.0 = Some((centre, 0.0));
        }
        if opens || shuts {
            let clip = if opens {
                Clip::DoorOpen
            } else {
                Clip::DoorClose
            };
            noises.write(Noise {
                clip: Some(clip),
                at: near_side(d, player.body.pos),
                loud: if leaf.robber && !player.body.is_crouching() {
                    LOUD
                } else {
                    0.0
                },
            });
        }
        // Coming at it fast, it is shoved open in time.
        let (goal, rate) = match opener {
            Some((_, _, gap, speed)) => (1.0, OPENING.min((gap / speed.max(1.0)).max(0.05))),
            None => (0.0, CLOSING),
        };
        leaf.open += (goal - was).clamp(-dt / rate, dt / rate);
        if leaf.open != was {
            moved.push((leaf.tile, leaf.open));
        }
    }
    if !moved.is_empty()
        && let Some(mut image) = images.get_mut(&world_materials.doors)
        && let Some(data) = image.data.as_mut()
    {
        for (tile, open) in moved {
            data[tile * 4] = ((1.0 - open) * 255.0) as u8;
        }
    }
    let Some(doors) = images
        .get(&world_materials.doors)
        .and_then(|i| i.data.as_deref())
    else {
        return;
    };
    let b = &player.body;
    for (mut leaf, mut t) in &mut leaves {
        if leaf.shown == leaf.open {
            continue;
        }
        let d = leaf.door;
        let at = near_side(d, b.pos);
        let reach = sight.robber_reach(b.pos, b.heading, controls.torch, at);
        if sight.seen(b.pos, at, reach, b.is_crouching(), doors) < 0.1 {
            continue;
        }
        leaf.shown = leaf.open;
        let s = leaf.shown;
        let a = s * s * (3.0 - 2.0 * s) * FRAC_PI_2;
        let dir = d.along * a.cos() + d.across * leaf.side * a.sin();
        t.rotation = Quat::from_rotation_z(dir.to_angle());
    }
}
