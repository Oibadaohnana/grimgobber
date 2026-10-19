//! Loose things about the bank (mugs, staplers, files, paper balls): a
//! poltergeist flings them at robbers as its sign. The server keeps where
//! each lies (`Loose`); a client flies it there when it moves, and hears it land.

use crate::movement::TILE;
use crate::paint;
use crate::render::{WorldMaterials, things_material};
use crate::sound::{Clip, Noise};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

/// How many lie about a bank.
pub const JUNK: usize = 30;
/// Tiles a second through the air.
const SPEED: f32 = 9.0;

#[derive(Component, Serialize, Deserialize)]
pub struct Loose {
    pub at: Vec2,
    look: u8,
    /// Where its last flight began, and how many it has had: a change flies it.
    from: Vec2,
    flights: u8,
}

impl Loose {
    pub fn new(at: Vec2, i: usize) -> Self {
        Loose {
            at,
            look: (i % paint::JUNK_LOOKS) as u8,
            from: at,
            flights: 0,
        }
    }

    pub fn fling(&mut self, to: Vec2) {
        self.from = self.at;
        self.at = to;
        self.flights = self.flights.wrapping_add(1);
    }
}

/// On a client: the flights shown, and seconds into the last.
#[derive(Component)]
struct Flight {
    flights: u8,
    t: f32,
}

pub struct JunkPlugin;

impl Plugin for JunkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, fly);
    }
}

pub fn dress(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    loose: Query<(Entity, &Loose), Added<Loose>>,
) {
    for (e, l) in &loose {
        let turn = paint::noise1(l.at.x * 0.7 + l.at.y) * TAU;
        commands.entity(e).insert((
            Flight {
                flights: l.flights,
                t: f32::MAX,
            },
            Mesh2d(meshes.add(paint::junk(l.look as usize))),
            MeshMaterial2d(things_material(&world_materials)),
            Transform::from_translation(l.at.extend(0.6))
                .with_rotation(Quat::from_rotation_z(turn)),
            Visibility::default(),
        ));
    }
}

/// Up in an arc, spinning, and down with a clatter.
fn fly(
    time: Res<Time>,
    mut loose: Query<(&Loose, &mut Flight, &mut Transform)>,
    mut noises: MessageWriter<Noise>,
) {
    for (l, mut f, mut t) in &mut loose {
        if f.flights != l.flights {
            f.flights = l.flights;
            f.t = 0.0;
        }
        let time_up = (l.from.distance(l.at) / (SPEED * TILE)).max(0.25);
        if f.t > time_up {
            continue;
        }
        f.t += time.delta_secs();
        let k = (f.t / time_up).min(1.0);
        t.translation = l
            .from
            .lerp(l.at, k)
            .extend(if k < 1.0 { 25.0 } else { 0.6 });
        t.scale = Vec3::splat(1.0 + 0.6 * (PI * k).sin());
        t.rotate_z(9.0 * time.delta_secs());
        if k >= 1.0 {
            noises.write(Noise {
                clip: Some(Clip::Thud),
                at: l.at,
                loud: 0.0,
            });
        }
    }
}
