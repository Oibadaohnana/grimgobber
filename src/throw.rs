//! Coins: in hand, a left click throws one at the pointer, up to `THROW`
//! tiles, stopping short of anything solid in the way. Landing, it plings,
//! guards `LOUD` tiles off hear it and go to look, and it lies on the floor
//! till one of them pockets it, or the robber picks it up again (E).

use crate::dev::Script;
use crate::guard::Guard;
use crate::inventory::{Inventory, Kind};
use crate::movement::{Solids, TILE};
use crate::paint;
use crate::player::{Player, Shown, Swing};
use crate::render::{WorldMaterials, WorldPart, things_material};
use crate::sound::{Clip, Noise};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::f32::consts::PI;

const THROW: f32 = 8.0;
const LOUD: f32 = 7.0;
/// Tiles a second through the air, and the shortest flight.
const SPEED: f32 = 10.0;
const QUICKEST: f32 = 0.2;
/// A guard this close to a coin on the floor picks it up; the robber, this
/// close, can.
const PICK: f32 = 0.7 * TILE;
const REACH: f32 = 0.8 * TILE;

#[derive(Component)]
struct Flying {
    from: Vec2,
    to: Vec2,
    age: f32,
    time: f32,
}

/// A coin on the floor.
#[derive(Component)]
struct Dropped;

#[derive(Resource)]
struct CoinMesh(Handle<Mesh>);

/// With coins in hand: the ring of how far they go, and where one would land.
#[derive(Component)]
enum Aim {
    Reach,
    Spot,
}

pub struct ThrowPlugin;

impl Plugin for ThrowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            (aim, throw, fly, pick_up, collect.after(crate::guard::knock))
                .chain()
                .run_if(resource_exists::<Player>),
        );
    }
}

/// The aim's marks, over everything and in any light (not the world's material).
fn spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(CoinMesh(meshes.add(paint::coin())));
    let flat = MeshMaterial2d(materials.add(ColorMaterial::default()));
    for (aim, mesh) in [
        (Aim::Reach, paint::ring(THROW * TILE)),
        (Aim::Spot, paint::target()),
    ] {
        commands.spawn((
            aim,
            Mesh2d(meshes.add(mesh)),
            flat.clone(),
            Transform::from_xyz(0.0, 0.0, 26.0),
            Visibility::Hidden,
        ));
    }
}

/// The pointer in the world.
fn pointer(
    window: &Query<&Window, With<PrimaryWindow>>,
    camera: &Query<(&Camera, &GlobalTransform)>,
) -> Option<Vec2> {
    let c = window.single().ok()?.cursor_position()?;
    let (camera, at) = camera.single().ok()?;
    camera.viewport_to_world_2d(at, c).ok()
}

/// Coins in hand, the ring round him and the spot under the pointer.
fn aim(
    inventory: Res<Inventory>,
    player: Res<Player>,
    shown: Res<Shown>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform)>,
    mut marks: Query<(&Aim, &mut Transform, &mut Visibility)>,
) {
    let on = !inventory.open && inventory.holds(Kind::Coin);
    let b = &player.body;
    let spot = pointer(&window, &camera).map(|p| {
        landing(
            b.pos + Vec2::from_angle(b.heading) * 16.0,
            p,
            &player.solids,
        )
    });
    for (aim, mut t, mut vis) in &mut marks {
        let at = match aim {
            Aim::Reach => Some(shown.pos),
            Aim::Spot => spot,
        };
        *vis = match at {
            Some(_) if on => Visibility::Inherited,
            _ => Visibility::Hidden,
        };
        if let Some(at) = at {
            t.translation = at.extend(26.0);
        }
    }
}

/// Where a coin thrown from `from` at `to` comes down: no further than
/// `THROW` tiles, and short of the first solid thing on the way.
fn landing(from: Vec2, to: Vec2, solids: &Solids) -> Vec2 {
    let to = from + (to - from).clamp_length_max(THROW * TILE);
    let n = (from.distance(to) / 4.0).ceil().max(1.0) as usize;
    let mut last = from;
    for k in 1..=n {
        let p = from.lerp(to, k as f32 / n as f32);
        if solids.solid_at(p) {
            return last;
        }
        last = p;
    }
    to
}

#[allow(clippy::too_many_arguments)]
fn throw(
    mouse: Res<ButtonInput<MouseButton>>,
    script: Res<Script>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform)>,
    player: Res<Player>,
    coin: Res<CoinMesh>,
    world_materials: Res<WorldMaterials>,
    mut inventory: ResMut<Inventory>,
    mut strike: ResMut<Swing>,
    mut commands: Commands,
) {
    let click = mouse.just_pressed(MouseButton::Left) || script.pressed("Throw");
    let Some(held) = inventory.held() else {
        return;
    };
    if inventory.open || !inventory.holds(Kind::Coin) || !click {
        return;
    }
    let b = &player.body;
    let ahead = Vec2::from_angle(b.heading);
    // At the pointer; scripted, as far as it goes the way he faces.
    let aim = match pointer(&window, &camera) {
        Some(p) if !script.pressed("Throw") => p,
        _ => b.pos + ahead * THROW * TILE,
    };
    let from = b.pos + ahead * 16.0;
    let to = landing(from, aim, &player.solids);
    strike.0 = Some(0.0);
    commands.spawn((
        WorldPart,
        Flying {
            from,
            to,
            age: 0.0,
            time: (from.distance(to) / (SPEED * TILE)).max(QUICKEST),
        },
        Mesh2d(coin.0.clone()),
        MeshMaterial2d(things_material(&world_materials)),
        Transform::from_translation(from.extend(25.0)),
    ));
    inventory.use_one(held);
}

/// Up and down again as it goes; landed, it plings.
fn fly(
    time: Res<Time>,
    mut coins: Query<(Entity, &mut Flying, &mut Transform)>,
    mut noises: MessageWriter<Noise>,
    mut commands: Commands,
) {
    for (e, mut f, mut t) in &mut coins {
        f.age += time.delta_secs();
        let share = (f.age / f.time).min(1.0);
        t.translation = f.from.lerp(f.to, share).extend(25.0);
        t.scale = Vec3::splat(1.0 + 0.8 * (PI * share).sin());
        if share >= 1.0 {
            t.translation.z = 3.0;
            commands.entity(e).remove::<Flying>().insert(Dropped);
            noises.write(Noise {
                clip: Some(Clip::Coin),
                at: f.to,
                loud: LOUD,
            });
        }
    }
}

fn pick_up(
    coins: Query<(Entity, &Transform), With<Dropped>>,
    mut guards: Query<&mut Guard>,
    mut commands: Commands,
) {
    for (e, t) in &coins {
        let at = t.translation.truncate();
        if let Some(mut g) = guards
            .iter_mut()
            .find(|g| g.awake() && g.body.pos.distance(at) < PICK)
        {
            g.pocket(Kind::Coin, 1);
            commands.entity(e).despawn();
        }
    }
}

/// The coin within his reach, if nothing else wants E: E picks it up.
fn collect(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<Script>,
    player: Res<Player>,
    coins: Query<(Entity, &Transform), With<Dropped>>,
    mut inventory: ResMut<Inventory>,
    mut prompt: ResMut<crate::prompt::Prompt>,
    mut commands: Commands,
) {
    if prompt.0.is_some() || inventory.open {
        return;
    }
    let pos = player.body.pos;
    let Some((e, at)) = coins
        .iter()
        .map(|(e, t)| (e, t.translation.truncate()))
        .find(|(_, at)| at.distance(pos) < REACH)
    else {
        return;
    };
    prompt.0 = Some((at + vec2(0.0, 28.0), "Pick up"));
    if (keys.just_pressed(KeyCode::KeyE) || script.pressed("Use"))
        && inventory.add(Kind::Coin, 1, None)
    {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::Aabb;

    #[test]
    fn a_coin_flies_its_reach_and_stops_short_of_a_wall() {
        let wall = Aabb::new(vec2(500.0, 0.0), vec2(552.0, 1000.0));
        let solids = Solids::new(Aabb::new(Vec2::ZERO, Vec2::splat(1000.0)), vec![wall]);
        let from = vec2(100.0, 100.0);
        let far = landing(from, vec2(100.0, 900.0), &solids);
        assert!((far.distance(from) - THROW * TILE).abs() < 1.0, "{far}");
        let short = landing(from, vec2(900.0, 100.0), &solids);
        assert!(short.x < 500.0 && short.x > 490.0, "{short}");
    }
}
