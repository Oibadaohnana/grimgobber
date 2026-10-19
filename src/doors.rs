//! Doors swing open, away from whoever comes: pushed by hand within reach
//! (the robber puts his arm out, and back as it shuts behind him), the glass
//! entrance by its sensor. How shut each is goes to the ray tracer, so a shut door stops
//! sight and lamplight. The leaf is drawn as the robber last saw it: in the
//! fog, nobody is seen opening doors. While the wraith haunts, the doors to
//! the outside slam shut and are locked (`Leaf::locked`, a red lock on
//! them): solid for the robbers (`Barred`) till it is over.

use crate::map::{Door, DoorKind, Map, tile_centre};
use crate::movement::{Aabb, BODY_MARGIN, Solids, TILE};
use crate::paint;
use crate::player::{Controls, Dead, Level, LocalPlayer, Player, Reach};
use crate::render::{WorldMaterials, WorldPart, things_material};
use crate::sight::Sight;
use crate::smash::Bank;
use crate::sound::{Clip, Noise};
use crate::wraith::Wraith;
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, Replicated};
use serde::{Deserialize, Serialize};
use std::f32::consts::FRAC_PI_2;

/// A hand-opened door: someone this close to the doorway's line, in front of it.
const REACH: f32 = BODY_MARGIN + 0.5 * TILE;
/// The entrance's sensor: this close to the doorway's middle.
const SENSOR: f32 = 1.6 * TILE;
const OPENING: f32 = 0.15;
const CLOSING: f32 = 0.4;
/// Swung by the wraith: open this long with nobody there, then slammed in so long.
const HAUNTED: f32 = 1.2;
const SLAM: f32 = 0.12;
/// How far off the robber at a door is heard, in tiles (crouched, not at all).
const LOUD: f32 = 4.0;

/// Per tile, as the doors texture holds it: how shut, lying along x, glass, 0.
#[derive(Resource)]
pub struct Shut(pub Vec<u8>);

impl Shut {
    pub fn new(map: &Map) -> Self {
        let mut shut = vec![0u8; map.tiles.len() * 4];
        for d in map.doors() {
            let i = map.idx(d.x, d.y) * 4;
            shut[i] = 255;
            shut[i + 1] = if d.along.x != 0.0 { 255 } else { 0 };
            shut[i + 2] = if d.kind == DoorKind::Glass { 255 } else { 0 };
        }
        Shut(shut)
    }
}

#[derive(Component, Serialize, Deserialize)]
pub struct Leaf {
    door: Door,
    tile: usize,
    open: f32,
    /// How open it is drawn.
    #[serde(skip)]
    shown: f32,
    /// Which way across the wall it swings, picked as it starts to open.
    side: f32,
    /// The robber who opened it, whose arm goes out to it.
    #[entities]
    robber: Option<Entity>,
    /// Seconds the wraith holds it open, then (below 0) slams it.
    #[serde(skip)]
    haunted: f32,
    /// Between inside and outside (the server's), and locked by a haunt.
    #[serde(skip)]
    outer: bool,
    pub locked: bool,
}

/// What the robbers walk against: the bank's solids and the locked doorways,
/// while any door is locked.
#[derive(Resource, Default)]
pub struct Barred(pub Option<Solids>);

/// The lock drawn on a locked door.
#[derive(Component)]
struct Lock;

impl Leaf {
    pub fn centre(&self) -> Vec2 {
        tile_centre(self.door.x, self.door.y)
    }

    pub fn is_shut(&self) -> bool {
        self.open == 0.0
    }

    pub fn haunt(&mut self) {
        self.haunted = HAUNTED;
    }
}

pub struct DoorsPlugin;

impl Plugin for DoorsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                swing
                    .run_if(resource_exists::<Level>.and_then(in_state(ClientState::Disconnected))),
                to_shut.run_if(resource_exists::<Shut>),
                bar.run_if(resource_exists::<Bank>),
            )
                .chain(),
        )
        .init_resource::<Barred>();
        if crate::drawn(app) {
            app.add_systems(
                Update,
                (upload, show, locks).after(to_shut).distributive_run_if(
                    resource_exists::<Shut>.and_then(resource_exists::<Sight>),
                ),
            );
        }
    }
}

pub fn spawn_doors(commands: &mut Commands, map: &Map) {
    for door in map.doors() {
        let side = |s: f32| {
            let p = tile_centre(door.x, door.y) + door.across * s * TILE;
            let (x, y) = crate::map::tile_of(p);
            map.inside(x, y) && map.at(x, y).indoor
        };
        commands.spawn((
            WorldPart,
            Replicated,
            Leaf {
                door,
                tile: map.idx(door.x, door.y),
                open: 0.0,
                shown: 0.0,
                side: 1.0,
                robber: None,
                haunted: 0.0,
                outer: side(1.0) != side(-1.0),
                locked: false,
            },
        ));
    }
}

pub fn dress(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    leaves: Query<(Entity, &Leaf), Added<Leaf>>,
) {
    for (e, leaf) in &leaves {
        let d = leaf.door;
        commands
            .entity(e)
            .insert((
                Mesh2d(meshes.add(paint::door_leaf(d.kind))),
                MeshMaterial2d(things_material(&world_materials)),
                Transform::from_translation(d.hinge.extend(5.0))
                    .with_rotation(Quat::from_rotation_z(d.along.to_angle())),
            ))
            .with_child((
                Lock,
                Mesh2d(meshes.add(paint::lock())),
                MeshMaterial2d(things_material(&world_materials)),
                Transform::from_xyz((TILE - 3.0) * 0.5, 0.0, 0.2),
                Visibility::Hidden,
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
    me: Option<Single<&Player, With<LocalPlayer>>>,
    mut robbers: Query<(Entity, &Player, &mut Reach), Without<Dead>>,
    mut leaves: Query<&mut Leaf>,
    wraiths: Query<&Wraith>,
    mut noises: MessageWriter<Noise>,
) {
    let haunting = wraiths.iter().any(|w| w.shown && !w.caught);
    let walkers: Vec<_> = robbers
        .iter()
        .map(|(e, p, _)| (Some(e), p.body.pos, p.body.speed, p.body.is_crouching()))
        .collect();
    let dt = time.delta_secs();
    for mut leaf in &mut leaves {
        let d = leaf.door;
        let centre = tile_centre(d.x, d.y);
        let by_hand = d.kind != DoorKind::Glass;
        let locked = leaf.outer && haunting;
        if leaf.locked != locked {
            leaf.locked = locked;
        }
        let opener = walkers
            .iter()
            .filter(|_| !locked)
            .find_map(|&(robber, pos, speed, _)| {
                let off = pos - centre;
                let gap = off.dot(d.across).abs();
                let near = if by_hand {
                    gap < REACH && off.dot(d.along).abs() < TILE * 0.5 + BODY_MARGIN
                } else {
                    off.length() < SENSOR
                };
                near.then_some((robber, off, gap, speed))
            });
        let ghost = leaf.haunted > 0.0 && !locked;
        let slam = leaf.haunted < 0.0;
        if leaf.haunted != 0.0 {
            let h = leaf.haunted;
            leaf.haunted = match h > 0.0 {
                true if h > dt => h - dt,
                true => -SLAM,
                false => (h + dt).min(0.0),
            };
        }
        let near = opener.is_some() || ghost;
        let was = leaf.open;
        if let Some((robber, off, _, _)) = opener
            && was == 0.0
        {
            leaf.side = if off.dot(d.across) > 0.0 { -1.0 } else { 1.0 };
            leaf.robber = robber;
        } else if ghost && was == 0.0 {
            // Nobody's arm goes out to it.
            leaf.robber = None;
        }
        let (opens, shuts) = (near && was == 0.0, !near && was == 1.0);
        if by_hand
            && (opens || shuts)
            && let Some((_, _, mut reach)) = leaf.robber.and_then(|e| robbers.get_mut(e).ok())
        {
            reach.0 = Some((centre, 0.0));
        }
        let quiet = walkers
            .iter()
            .find(|w| w.0.is_some() && w.0 == leaf.robber)
            .is_none_or(|w| w.3);
        if opens || shuts {
            let clip = if opens {
                Clip::DoorOpen
            } else {
                Clip::DoorClose
            };
            let ear = me.as_ref().map_or(centre, |p| p.body.pos);
            noises.write(Noise {
                clip: Some(clip),
                at: near_side(d, ear),
                loud: if quiet { 0.0 } else { LOUD },
            });
        }
        // Coming at it fast, it is shoved open in time.
        let (goal, rate) = match opener {
            Some((_, _, gap, speed)) => (1.0, OPENING.min((gap / speed.max(1.0)).max(0.05))),
            None if ghost => (1.0, OPENING),
            None if slam || locked => (0.0, SLAM),
            None => (0.0, CLOSING),
        };
        // Untouched unless it moves: a changed leaf is sent to every player.
        let open = was + (goal - was).clamp(-dt / rate, dt / rate);
        if open != was {
            leaf.open = open;
        }
    }
}

/// The robbers' solids again whenever the locked doors change.
fn bar(
    bank: Res<Bank>,
    leaves: Query<&Leaf>,
    mut barred: ResMut<Barred>,
    mut was: Local<Vec<usize>>,
) {
    let locked: Vec<usize> = leaves.iter().filter(|l| l.locked).map(|l| l.tile).collect();
    if locked == *was && !bank.is_changed() {
        return;
    }
    let map = &bank.0;
    barred.0 = (!locked.is_empty()).then(|| {
        let mut rects = map.solids();
        rects.extend(leaves.iter().filter(|l| l.locked).map(|l| {
            let c = l.centre();
            Aabb::new(c - Vec2::splat(TILE * 0.5), c + Vec2::splat(TILE * 0.5))
        }));
        Solids::new(map.bounds(), rects)
    });
    *was = locked;
}

/// A locked door shows its lock.
fn locks(
    leaves: Query<(&Leaf, &Children), Changed<Leaf>>,
    mut marks: Query<&mut Visibility, With<Lock>>,
) {
    for (leaf, children) in &leaves {
        for &c in children {
            if let Ok(mut vis) = marks.get_mut(c) {
                vis.set_if_neq(if leaf.locked {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}

fn to_shut(mut shut: ResMut<Shut>, leaves: Query<&Leaf, Changed<Leaf>>) {
    for leaf in &leaves {
        shut.0[leaf.tile * 4] = ((1.0 - leaf.open) * 255.0) as u8;
    }
}

fn upload(
    shut: Res<Shut>,
    world_materials: Res<WorldMaterials>,
    mut images: ResMut<Assets<Image>>,
) {
    if shut.is_changed()
        && let Some(mut image) = images.get_mut(&world_materials.doors)
    {
        image.data = Some(shut.0.clone());
    }
}

fn show(
    me: Single<(&Player, &Controls), With<LocalPlayer>>,
    sight: Res<Sight>,
    shut: Res<Shut>,
    mut leaves: Query<(&mut Leaf, &mut Transform)>,
) {
    let doors = &shut.0;
    let (player, controls) = *me;
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
