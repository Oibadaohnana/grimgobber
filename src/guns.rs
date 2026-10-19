//! Guns. The hunter's pistol, in hand, aims at the pointer as bims' does
//! (`player::gun_line`), a reticle on it; a left click fires, R reloads from
//! his rounds. A shot stops at walls, windows, tall things and shut doors,
//! or at the first body it reaches, and is heard `LOUD` tiles off. The
//! server traces it again for the wraith (`capture::Shot`).

use crate::doors::Shut;
use crate::inventory::{Inventory, Kind};
use crate::movement::TILE;
use crate::paint::{self, hex};
use crate::player::{Controls, Level, Living, LocalPlayer, Player, gun_line};
use crate::sight::Sight;
use crate::sound::{Clip, Noise};
use bevy::prelude::*;

const MAG: u32 = 8;
const RELOAD: f32 = 1.4;
const EVERY: f32 = 0.22;
const RANGE: f32 = 20.0;
pub const LOUD: f32 = 26.0;
/// How near a body's middle a shot must pass to hit it.
const HIT: f32 = 16.0;
/// The hunter's pistol's state.
#[derive(Component, Default)]
pub struct Gunplay {
    /// Seconds of a reload left (0: none), and till the next shot.
    pub reload: f32,
    cool: f32,
}

#[cfg(feature = "client")]
#[derive(Resource)]
struct Looks {
    tracer: Handle<Mesh>,
    flash: Handle<Mesh>,
    flat: Handle<ColorMaterial>,
}

/// A streak or flash, gone in this many seconds.
#[derive(Component)]
struct Fade(f32);

/// The reticle: within the pistol's reach, or past it.
#[derive(Component)]
struct Reticle(bool);

pub struct GunsPlugin;

#[cfg(feature = "client")]
impl Plugin for GunsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            Update,
            (holster, trigger, fade, reticle)
                .chain()
                .distributive_run_if(resource_exists::<Level>.and_then(resource_exists::<Sight>)),
        );
    }
}

#[cfg(feature = "client")]
fn spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let flat = materials.add(ColorMaterial::default());
    for (inside, colour) in [(true, hex(0xf0ead8)), (false, hex(0x80868c))] {
        commands.spawn((
            Reticle(inside),
            Mesh2d(meshes.add(paint::reticle(colour.alpha(0.85)))),
            MeshMaterial2d(flat.clone()),
            Transform::from_xyz(0.0, 0.0, 28.0),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(Looks {
        tracer: meshes.add(paint::tracer()),
        flash: meshes.add(paint::flash()),
        flat,
    });
}

/// A shot from `from` along `dir`: where it stops, and which of `bodies`
/// (their middles) it hit there, if one.
pub fn shoot(
    sight: &Sight,
    doors: &[u8],
    from: Vec2,
    dir: Vec2,
    bodies: &[Vec2],
) -> (Vec2, Option<usize>) {
    let steps = (RANGE * TILE / 4.0) as usize;
    for k in 1..=steps {
        let p = from + dir * (k as f32 * 4.0);
        if let Some(i) = bodies.iter().position(|b| b.distance(p) < HIT) {
            return (p, Some(i));
        }
        if sight.stops(p, doors) {
            return (p, None);
        }
    }
    (from + dir * RANGE * TILE, None)
}

/// The streak from the muzzle to where it stopped, and the flash.
#[cfg(feature = "client")]
fn streak(commands: &mut Commands, looks: &Looks, from: Vec2, to: Vec2) {
    let d = to - from;
    commands.spawn((
        Fade(0.07),
        Mesh2d(looks.tracer.clone()),
        MeshMaterial2d(looks.flat.clone()),
        Transform::from_translation(from.extend(27.0))
            .with_rotation(Quat::from_rotation_z(d.to_angle()))
            .with_scale(vec3(d.length(), 1.0, 1.0)),
    ));
    commands.spawn((
        Fade(0.05),
        Mesh2d(looks.flash.clone()),
        MeshMaterial2d(looks.flat.clone()),
        Transform::from_translation(from.extend(27.0)),
    ));
}

/// Out and away again: the pistol's sounds, a reload cut short.
fn holster(
    me: Single<(&Player, &Inventory, &mut Gunplay), With<LocalPlayer>>,
    mut was: Local<bool>,
    mut noises: MessageWriter<Noise>,
) {
    let (player, inventory, mut gun) = me.into_inner();
    let held = inventory.holds(Kind::Pistol);
    if held != *was {
        let clip = if held { Clip::Unholster } else { Clip::Holster };
        noises.write(Noise {
            clip: Some(clip),
            at: player.body.pos,
            loud: 0.0,
        });
        gun.reload = 0.0;
    }
    *was = held;
}

/// The hunter's pistol: R reloads, a left click fires.
#[cfg(feature = "client")]
#[allow(clippy::too_many_arguments)]
fn trigger(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    script: Res<crate::dev::Script>,
    me: Single<(&Controls, &Player, &mut Inventory, &mut Gunplay), Living>,
    sight: Res<Sight>,
    looks: Res<Looks>,
    shut: Res<Shut>,
    mut noises: MessageWriter<Noise>,
    mut shots: MessageWriter<crate::capture::Shot>,
    mut commands: Commands,
) {
    let (controls, player, mut inventory, mut gun) = me.into_inner();
    let dt = time.delta_secs();
    gun.cool -= dt;
    let b = &player.body;
    let Some(id) = inventory.held().filter(|_| inventory.holds(Kind::Pistol)) else {
        return;
    };
    let rounds = inventory.items.iter().position(|i| i.kind == Kind::Rounds);
    if gun.reload > 0.0 {
        gun.reload -= dt;
        if gun.reload <= 0.0
            && let Some(r) = rounds
        {
            let n = (MAG - inventory.items[id].count).min(inventory.items[r].count);
            inventory.items[id].count += n;
            inventory.items[r].count -= n;
            if inventory.items[r].count == 0 {
                inventory.remove(r);
            }
        }
        return;
    }
    if inventory.open {
        return;
    }
    if (keys.just_pressed(KeyCode::KeyR) || script.pressed("Reload"))
        && inventory.items[id].count < MAG
        && rounds.is_some()
    {
        gun.reload = RELOAD;
        noises.write(Noise {
            clip: Some(Clip::Reload),
            at: b.pos,
            loud: 0.0,
        });
        return;
    }
    let click = mouse.just_pressed(MouseButton::Left) || script.pressed("Fire");
    if !click
        || gun.cool > 0.0
        || inventory.items[id].count == 0
        || b.is_rolling()
        || b.is_sprinting()
    {
        return;
    }
    let pointer = controls
        .pointer
        .filter(|_| !script.pressed("Fire"))
        .unwrap_or(b.pos + Vec2::from_angle(b.heading) * TILE * 4.0);
    let (from, dir) = gun_line(b.pos, b.heading, pointer);
    let (to, _) = shoot(&sight, &shut.0, from, dir, &[]);
    streak(&mut commands, &looks, from, to);
    // Heard by the wraith on the server, from the `Shot`.
    noises.write(Noise {
        clip: Some(Clip::Shot),
        at: from,
        loud: 0.0,
    });
    shots.write(crate::capture::Shot { from, dir });
    inventory.items[id].count -= 1;
    gun.cool = EVERY;
}

fn fade(time: Res<Time>, mut fading: Query<(Entity, &mut Fade)>, mut commands: Commands) {
    let dt = time.delta_secs();
    for (e, mut f) in &mut fading {
        f.0 -= dt;
        if f.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

/// On the pointer while the pistol is in hand: light within its reach, grey past it.
fn reticle(
    me: Single<(&Inventory, &Controls, &Player), With<LocalPlayer>>,
    mut marks: Query<(&Reticle, &mut Transform, &mut Visibility)>,
) {
    let (inventory, controls, player) = *me;
    let on = !inventory.open && inventory.holds(Kind::Pistol);
    let b = &player.body;
    for (mark, mut t, mut vis) in &mut marks {
        let at = controls
            .pointer
            .filter(|p| on && (p.distance(b.pos) <= RANGE * TILE) == mark.0);
        *vis = if at.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(at) = at {
            t.translation = at.extend(28.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shot_hits_the_first_body_and_stops_at_walls() {
        let map = crate::map::generate(6);
        let sight = Sight::new(&map);
        // Along the pavement: the nearer of two bodies takes it.
        let from = crate::map::tile_centre(24, 8);
        let bodies = [from + vec2(6.0 * TILE, 0.0), from + vec2(3.0 * TILE, 0.0)];
        assert_eq!(shoot(&sight, &[], from, Vec2::X, &bodies).1, Some(1));
        // Up into the bank's brick front, nobody behind it is hit.
        let wall = crate::map::tile_centre(21, 8);
        let behind = [wall + vec2(0.0, 3.0 * TILE)];
        let (to, hit) = shoot(&sight, &[], wall, Vec2::Y, &behind);
        assert!(hit.is_none() && to.y < wall.y + 1.5 * TILE, "{to}");
    }
}
