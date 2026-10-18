use crate::dev::Script;
use crate::guard::Guard;
use crate::guns::Gunplay;
use crate::inventory::{Inventory, Kind};
use crate::map::Map;
use crate::movement::{
    Aabb, BODY_MARGIN, Body, MARCH_SPEED, STEP, Solids, Steer, TILE, approach, wrap_angle,
};
use crate::net::Owner;
use crate::paint;
use crate::render::{
    MainCamera, WorldMaterial, WorldMaterials, WorldPart, things_material, write_params,
};
use bevy::ecs::query::QueryItem;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::{Remote, Replicated};
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

#[derive(Component, Serialize, Deserialize)]
#[require(Controls, Inventory, Reach, Swing, Gunplay)]
pub struct Player {
    pub body: Body,
}

#[derive(Component)]
pub struct LocalPlayer;

#[derive(Resource)]
pub struct Level {
    pub solids: Solids,
    /// The half-height props' bodies.
    pub cover: Vec<Aabb>,
    pub hideouts: Vec<Aabb>,
}

impl Level {
    pub fn new(map: &Map) -> Self {
        let props =
            |f: fn(&crate::map::Prop) -> Option<Aabb>| map.props.iter().filter_map(f).collect();
        Level {
            solids: Solids::new(map.bounds(), map.solids()),
            cover: props(|p| p.kind.is_low().then_some(p.body)),
            hideouts: props(|p| p.kind.is_hideout().then_some(p.block)),
        }
    }

    pub fn hideout(&self, at: Vec2) -> Option<Aabb> {
        self.hideouts.iter().copied().find(|h| h.contains(at))
    }

    pub fn in_cover(&self, b: &Body) -> bool {
        b.is_crouching()
            && self
                .cover
                .iter()
                .any(|c| c.expand(BODY_MARGIN + TILE * 0.5).contains(b.pos))
    }
}

#[derive(Component, Default, Serialize, Deserialize)]
pub struct Controls {
    pub walk: Option<f32>,
    pub aim: f32,
    pub sprint: bool,
    /// A roll asked for and not yet started.
    pub roll: bool,
    pub torch: bool,
    pub crouch: bool,
    /// The pointer in the world, while it is in the window.
    pub pointer: Option<Vec2>,
}

/// Drawn between fixed steps.
#[derive(Component, Default)]
pub struct Shown {
    pub pos: Vec2,
    crouch: f32,
}

/// A hand put out to a door as it opens or shuts: where, and how long ago.
#[derive(Component, Default)]
pub struct Reach(pub Option<(Vec2, f32)>);

const REACH_TIME: f32 = 0.4;

/// The pistol held in both hands this far out from his chest, its barrel
/// this long, swung onto the pointer within `AIM_ACROSS` of his facing
/// (bims').
const GRIP: f32 = 20.0;
const GUN_LENGTH: f32 = 16.0;
const AIM_ACROSS: f32 = 1.15;

/// The aim's angle in his frame, the pointer `off` him in his frame.
fn aim_at(off: Vec2) -> f32 {
    off.to_angle().clamp(-AIM_ACROSS, AIM_ACROSS)
}

/// The two-handed grip aimed at `a`: his right hand on the grip, his left
/// cupped under it.
fn grip(a: f32) -> (Vec2, Vec2) {
    let along = Vec2::from_angle(a);
    let at = along * GRIP;
    (
        at - along.perp() * 1.5,
        at - along * 2.0 + along.perp() * 3.0,
    )
}

/// Where a shot leaves his pistol and which way, at `pos` facing `heading`:
/// the grip is on the line from his chest to the pointer, and the barrel
/// runs on along it.
pub fn gun_line(pos: Vec2, heading: f32, pointer: Vec2) -> (Vec2, Vec2) {
    let turn = Vec2::from_angle(heading);
    let along = Vec2::from_angle(aim_at(Vec2::from_angle(-heading).rotate(pointer - pos)));
    (
        pos + turn.rotate(along * (GRIP + GUN_LENGTH)),
        turn.rotate(along),
    )
}

/// A swing of the crowbar, how long ago.
#[derive(Component, Default)]
pub struct Swing(pub Option<f32>);

const SWING_TIME: f32 = 0.35;

#[derive(Component, Clone, Copy)]
pub enum Part {
    Foot(i8),
    Knee(i8),
    Bag,
    Torso,
    Arm(i8),
    Hand(i8),
    Lens,
    /// Drawn while it is in his hands.
    Tool(Kind),
    Head,
    /// The pistol, held out in both hands.
    Gun,
    Streak(i8),
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_seconds(STEP as f64))
            .add_systems(FixedUpdate, step.run_if(resource_exists::<Level>));
        if crate::drawn(app) {
            app.add_systems(PreUpdate, read_controls.after(bevy::input::InputSystems))
                .add_systems(Update, (show, animate, follow, zoom).chain());
        }
    }
}

pub fn spawn_robber(commands: &mut Commands, at: Vec2, local: bool) -> Entity {
    let robber = commands
        .spawn((
            WorldPart,
            Player {
                body: Body::new(at, std::f32::consts::FRAC_PI_2),
            },
            Replicated,
        ))
        .id();
    if local {
        commands.entity(robber).insert(LocalPlayer);
    }
    robber
}

/// His picture, fading unless he is the one looking.
pub fn dress(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    robbers: Query<(Entity, &Player, Has<LocalPlayer>), Added<Player>>,
) {
    for (robber, player, local) in &robbers {
        let at = player.body.pos;
        let p = paint::robber(!local);
        let [foot, knee, arm, glove, crowbar, torch, lens, streak] = [
            p.foot, p.knee, p.arm, p.glove, p.crowbar, p.torch, p.lens, p.streak,
        ]
        .map(|m| Mesh2d(meshes.add(m)));
        let coin = Mesh2d(meshes.add(paint::coin()));
        let pistol = Mesh2d(meshes.add(paint::pistol()));
        let mat = MeshMaterial2d(things_material(&world_materials));
        commands
            .entity(robber)
            .insert((
                Shown {
                    pos: at,
                    crouch: 0.0,
                },
                Transform::from_xyz(at.x, at.y, 10.0),
                Visibility::default(),
            ))
            .with_children(|r| {
                r.spawn((
                    Mesh2d(meshes.add(p.shadow)),
                    mat.clone(),
                    Transform::from_xyz(0.0, 0.0, -0.9),
                ));
                for side in [-1i8, 1] {
                    r.spawn((
                        Part::Foot(side),
                        foot.clone(),
                        mat.clone(),
                        Transform::from_xyz(0.0, 8.0 * side as f32, -0.6),
                    ));
                    r.spawn((
                        Part::Knee(side),
                        knee.clone(),
                        mat.clone(),
                        Transform::from_xyz(0.0, 0.0, -0.3).with_scale(Vec3::ZERO),
                    ));
                }
                r.spawn((
                    Part::Torso,
                    Mesh2d(meshes.add(p.torso)),
                    mat.clone(),
                    Transform::from_xyz(-1.0, 0.0, 0.0),
                ));
                r.spawn((
                    Part::Bag,
                    Mesh2d(meshes.add(p.bag)),
                    mat.clone(),
                    Transform::from_xyz(-13.0, 0.0, 0.2),
                ));
                for side in [-1i8, 1] {
                    r.spawn((
                        Part::Hand(side),
                        glove.clone(),
                        mat.clone(),
                        Transform::from_xyz(16.0, 14.0 * side as f32, 0.36),
                    ));
                    let hand = Transform::from_xyz(15.0, 0.0, 0.05);
                    r.spawn((
                        Part::Arm(side),
                        arm.clone(),
                        mat.clone(),
                        Transform::from_xyz(1.0, 14.0 * side as f32, 0.3),
                    ))
                    .with_children(|a| {
                        if side < 0 {
                            a.spawn((
                                Part::Tool(Kind::Crowbar),
                                crowbar.clone(),
                                mat.clone(),
                                hand.with_rotation(Quat::from_rotation_z(0.35)),
                            ));
                            a.spawn((
                                Part::Tool(Kind::Coin),
                                coin.clone(),
                                mat.clone(),
                                hand.with_translation(vec3(17.0, 0.0, 0.1)),
                            ));
                        } else {
                            a.spawn((
                                Part::Tool(Kind::Flashlight),
                                torch.clone(),
                                mat.clone(),
                                hand.with_rotation(Quat::from_rotation_z(0.42)),
                            ))
                            .with_children(|t| {
                                t.spawn((
                                    Part::Lens,
                                    lens.clone(),
                                    mat.clone(),
                                    Transform::from_xyz(0.0, 0.0, 0.01),
                                    Visibility::Hidden,
                                ));
                            });
                        }
                    });
                }
                r.spawn((
                    Part::Gun,
                    pistol.clone(),
                    mat.clone(),
                    Transform::from_xyz(GRIP, 0.0, 0.33),
                    Visibility::Hidden,
                ));
                r.spawn((
                    Part::Head,
                    Mesh2d(meshes.add(p.head)),
                    mat.clone(),
                    Transform::from_xyz(2.0, 0.0, 0.5),
                ));
                for k in [-1i8, 0, 1] {
                    r.spawn((
                        Part::Streak(k),
                        streak.clone(),
                        mat.clone(),
                        Transform::from_xyz(-24.0, 9.0 * k as f32, -0.8),
                        Visibility::Hidden,
                    ));
                }
            });
    }
}

fn read_controls(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<Script>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    me: Single<(&Shown, &Inventory, &mut Controls), With<LocalPlayer>>,
) {
    let (shown, inventory, mut controls) = me.into_inner();
    let held = |k: KeyCode, name: &str| keys.pressed(k) || script.held(name);
    let mut d = Vec2::ZERO;
    if held(KeyCode::KeyW, "W") || held(KeyCode::ArrowUp, "Up") {
        d.y += 1.0;
    }
    if held(KeyCode::KeyS, "S") || held(KeyCode::ArrowDown, "Down") {
        d.y -= 1.0;
    }
    if held(KeyCode::KeyA, "A") || held(KeyCode::ArrowLeft, "Left") {
        d.x -= 1.0;
    }
    if held(KeyCode::KeyD, "D") || held(KeyCode::ArrowRight, "Right") {
        d.x += 1.0;
    }
    controls.walk = (d != Vec2::ZERO).then(|| d.y.atan2(d.x));
    controls.sprint = held(KeyCode::ShiftLeft, "Shift") || keys.pressed(KeyCode::ShiftRight);
    if keys.just_pressed(KeyCode::KeyC) || script.pressed("Crouch") {
        controls.crouch = !controls.crouch;
    }
    if controls.sprint && controls.walk.is_some() {
        controls.crouch = false;
    }
    if keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::AltLeft)
        || keys.just_pressed(KeyCode::AltRight)
        || script.pressed("Roll")
    {
        controls.roll = true;
    }
    if keys.just_pressed(KeyCode::KeyF) || script.pressed("Torch") {
        controls.torch = !controls.torch;
    }
    // Only a flashlight in his hand shines; put away, it goes off.
    controls.torch &= inventory.holds(Kind::Flashlight);
    // The pointer is the inventory's while it is open.
    if inventory.open {
        return;
    }
    if let Some(aim) = script.aim {
        controls.aim = aim;
        return;
    }
    let (Ok(window), Ok((camera, at))) = (window.single(), camera.single()) else {
        return;
    };
    controls.pointer = window
        .cursor_position()
        .and_then(|c| camera.viewport_to_world_2d(at, c).ok());
    if let Some(world) = controls.pointer {
        let off = world - shown.pos;
        if off.length() > 1.0 {
            controls.aim = off.y.atan2(off.x);
        }
    }
}

/// The robbers this machine steps: its own, and the test crew where they live.
type Driven = Or<(With<LocalPlayer>, (Without<Remote>, Without<Owner>))>;

fn step(level: Res<Level>, mut players: Query<(&mut Player, &mut Controls), Driven>) {
    for (mut player, mut controls) in &mut players {
        let thick = level.hideout(player.body.pos).is_some();
        let b = &mut player.body;
        b.set_steer(Steer {
            walk: controls.walk,
            aim: controls.aim,
            sprint: controls.sprint,
            crouch: controls.crouch,
            thick,
        });
        if controls.roll {
            controls.roll = false;
            b.start_roll();
        }
        b.update(STEP, &level.solids);
    }
}

fn show(
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut players: Query<(&Player, &mut Shown, Has<Remote>, Has<LocalPlayer>)>,
) {
    for (player, mut shown, remote, local) in &mut players {
        let b = &player.body;
        // Another player's steps come in on the line's time, not ours.
        shown.pos = if remote && !local && b.pos.distance(shown.pos) < 2.0 * TILE {
            shown.pos + (b.pos - shown.pos) * approach(20.0, time.delta_secs())
        } else if b.pos.distance(b.was) > 52.0 {
            b.pos
        } else {
            b.was + (b.pos - b.was) * fixed.overstep_fraction()
        };
    }
}

type Robber<'a> = (
    Entity,
    &'a Player,
    &'a Controls,
    &'a Inventory,
    &'a mut Shown,
    &'a mut Reach,
    &'a mut Swing,
    &'a mut Transform,
);

fn animate(
    time: Res<Time>,
    mut robbers: Query<Robber, Without<Part>>,
    children: Query<&Children>,
    mut parts: Query<(&Part, &mut Transform, &mut Visibility), Without<Player>>,
) {
    for robber in &mut robbers {
        pose(&time, robber, &children, &mut parts);
    }
}

fn pose(
    time: &Time,
    (root, player, controls, inventory, mut shown, mut reach, mut strike, mut t): QueryItem<Robber>,
    children: &Query<&Children>,
    parts: &mut Query<(&Part, &mut Transform, &mut Visibility), Without<Player>>,
) {
    let b = &player.body;
    let rolling = b.roll_share();
    reach.0 = reach.0.map(|(at, age)| (at, age + time.delta_secs()));
    reach.0 = reach
        .0
        .filter(|&(_, age)| age < REACH_TIME && rolling.is_none());
    strike.0 = strike
        .0
        .map(|age| age + time.delta_secs())
        .filter(|&age| age < SWING_TIME && rolling.is_none());
    // The arm on the door's side turns to it and stretches, and back.
    let push = reach.0.map(|(at, age)| {
        let rel = wrap_angle((at - shown.pos).to_angle() - b.heading);
        (rel, (PI * age / REACH_TIME).sin())
    });
    t.translation = shown.pos.extend(10.0);
    t.rotation = Quat::from_rotation_z(b.heading);
    let down = if b.is_crouching() { 1.0 } else { 0.0 };
    shown.crouch += (down - shown.crouch) * approach(14.0, time.delta_secs());
    // A roll tucks into the crouch pose, quickly, and back out at the end.
    let tuck = rolling.map_or(0.0, |s| ((PI * s).sin() * 2.0).min(1.0));
    let c = shown.crouch.max(tuck);
    // Crouched from above: folded forward, knees out in front, a kneeling stance.
    let m = |stand: f32, low: f32| stand + (low - stand) * c;
    t.scale = Vec3::splat(m(1.0, 0.92) * (1.0 - 0.12 * tuck));
    // Rolling, every part turns head over heels about the axis across the
    // body: a part `up` above the middle goes forward, under, and back over
    // the top. Draw order follows height.
    let (sin, cos) = rolling.map_or((0.0, 1.0), |s| (s * TAU).sin_cos());
    let flip = |pt: &mut Transform, x: f32, up: f32, z: f32| {
        pt.translation.x = x * cos + up * sin;
        pt.translation.z = if rolling.is_some() {
            (up * cos - x * sin) * 0.03
        } else {
            z
        };
    };

    let sprint = b.is_sprinting();
    // The pistol in hand (not mid-roll): at the pointer, or ahead.
    let aim = (inventory.holds(Kind::Pistol) && rolling.is_none()).then(|| {
        controls.pointer.map_or(0.0, |p| {
            aim_at(Vec2::from_angle(-b.heading).rotate(p - shown.pos))
        })
    });
    let pace = if rolling.is_some() {
        0.0
    } else {
        (b.speed / MARCH_SPEED).clamp(0.0, 1.0)
    };
    let s = b.stride.sin();
    let reach = if sprint { 15.0 } else { 9.5 };
    let clock = time.elapsed_secs();
    let mut own = parts.iter_many_mut(children.iter_descendants(root));
    while let Some((part, mut pt, mut vis)) = own.fetch_next() {
        match *part {
            Part::Foot(side) => {
                let sd = side as f32;
                let shuffle = s * sd * 4.0 * pace;
                flip(
                    &mut pt,
                    m(s * sd * reach * pace, -1.0 - 9.0 * sd + shuffle),
                    -14.0,
                    -0.6,
                );
                pt.translation.y = m(8.0, 10.0) * sd;
            }
            Part::Knee(side) => {
                let sd = side as f32;
                flip(&mut pt, m(4.0, 23.0) + s * sd * 2.0 * pace, 0.0, -0.3);
                pt.translation.y = 7.5 * sd;
                pt.scale = Vec3::splat(c);
            }
            Part::Arm(side) | Part::Hand(side) => {
                let sd = side as f32;
                let swing = if sprint { 0.55 } else { m(0.14, 0.05) };
                let mut a = m(-0.42, -0.2) * sd - s * sd * swing * pace;
                let mut x = if sprint {
                    1.0 - s * sd * 3.0
                } else {
                    m(1.0, 6.0)
                };
                let mut k = 0.0;
                if let Some((rel, kk)) = push.filter(|&(rel, _)| (rel > 0.0) == (sd > 0.0)) {
                    a += (rel.clamp(-2.4, 2.4) - a) * kk;
                    x += 5.0 * kk;
                    k = kk;
                }
                if aim.is_some() {
                    x = 3.0;
                }
                // The crowbar's swing: out to his side, then hard across.
                if let Some(age) = strike.0
                    && side < 0
                {
                    let t = age / SWING_TIME;
                    a += sd * (TAU * t).sin() * 1.1;
                    x += 4.0 * (PI * t).sin();
                }
                let shoulder = vec2(x, m(14.0, 13.0) * sd);
                let out = shoulder + Vec2::from_angle(a) * 15.0;
                // An empty hand hangs by his hip, swung against his step (on
                // his knee, crouched), unless it goes out to a door.
                let held = if side < 0 {
                    [Kind::Crowbar, Kind::Coin, Kind::Pistol]
                        .iter()
                        .any(|&k| inventory.holds(k))
                } else {
                    inventory.holds(Kind::Flashlight)
                };
                let hand = if held {
                    out
                } else {
                    let pump = if sprint { 10.0 } else { 6.0 };
                    let hang = vec2(m(-2.0 - s * sd * pump * pace, 19.0), m(19.5, 9.5) * sd);
                    hang.lerp(out, k)
                };
                // The pistol held out in both hands.
                let hand = aim.map_or(hand, |a| {
                    let (right, left) = grip(a);
                    if side < 0 { right } else { left }
                });
                // Head over heels, as `flip` turns the rest.
                let fx = |v: f32| v * cos + 8.0 * sin;
                let z = if rolling.is_some() {
                    (8.0 * cos - shoulder.x * sin) * 0.03
                } else {
                    0.3
                };
                if matches!(*part, Part::Hand(_)) {
                    pt.translation = vec3(fx(hand.x), hand.y, z + 0.06);
                } else {
                    let d = vec2(fx(hand.x) - fx(shoulder.x), hand.y - shoulder.y);
                    pt.translation = vec3(fx(shoulder.x), shoulder.y, z);
                    pt.rotation = Quat::from_rotation_z(d.to_angle());
                    pt.scale.x = d.length() / 15.0;
                }
            }
            Part::Torso => {
                pt.rotation = Quat::from_rotation_z(0.07 * s * pace);
                flip(&mut pt, m(-1.0, 3.0), 2.0, 0.0);
                // Mid-roll it shows its back or chest, longer than the shoulders.
                let long = cos.abs() + 1.7 * sin.abs();
                pt.scale = vec3(m(1.0, 1.25) * long, m(1.0, 0.85), 1.0);
            }
            Part::Bag => {
                let bob = 1.0 + 0.04 * (2.0 * b.stride).sin() * pace;
                pt.scale = Vec3::new(bob, 1.0 / bob, 1.0) * m(1.0, 0.92);
                pt.rotation = Quat::from_rotation_z(-0.1 * s * pace);
                flip(&mut pt, m(-13.0, -6.0), 6.0, 0.2);
            }
            Part::Gun => {
                *vis = if aim.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if let Some(a) = aim {
                    pt.translation = (Vec2::from_angle(a) * GRIP).extend(0.33);
                    pt.rotation = Quat::from_rotation_z(a);
                }
            }
            Part::Head => {
                let x = if sprint { 4.5 } else { m(2.0, 11.0) };
                flip(&mut pt, x, 16.0, 0.5);
                pt.scale = Vec3::splat(m(1.0, 0.9));
            }
            Part::Streak(k) => {
                *vis = if sprint && rolling.is_none() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                let flicker = 0.6 + 0.4 * ((clock * 30.0 + k as f32 * 2.1).sin() * 0.5 + 0.5);
                pt.scale = Vec3::new(flicker * (1.0 + 0.4 * (k as f32).abs()), 1.0, 1.0);
            }
            Part::Tool(kind) => {
                *vis = if inventory.holds(kind) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
            Part::Lens => {
                *vis = if controls.torch {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn follow(
    me: Single<(Entity, &Shown, &Player, &Controls), With<LocalPlayer>>,
    robbers: Query<(Entity, &Shown, &Player, &Controls)>,
    guards: Query<&Guard>,
    time: Res<Time<Fixed>>,
    mut camera: Query<&mut Transform, (With<MainCamera>, Without<Player>)>,
    world_materials: Res<WorldMaterials>,
    mut materials: ResMut<Assets<WorldMaterial>>,
) {
    let (me, shown, player, controls) = *me;
    // His own first, then the nearest, as many as the shader takes.
    let mut lit: Vec<_> = robbers.iter().filter(|r| r.3.torch).collect();
    let far = |r: &(Entity, &Shown, _, _)| (r.0 != me, r.1.pos.distance_squared(shown.pos));
    lit.sort_by(|a, b| far(a).partial_cmp(&far(b)).unwrap());
    let torches: Vec<_> = lit
        .iter()
        .map(|(_, s, p, _)| {
            s.pos
                .extend(p.body.heading.cos())
                .extend(p.body.heading.sin())
        })
        .collect();
    if let Ok(mut c) = camera.single_mut() {
        c.translation.x = shown.pos.x;
        c.translation.y = shown.pos.y;
    }
    write_params(
        shown.pos,
        player.body.heading,
        controls.torch,
        player.body.is_crouching(),
        guards
            .iter()
            .next()
            .map_or(Vec4::ZERO, |g| g.view(time.overstep_fraction())),
        &torches,
        &world_materials,
        &mut materials,
    );
}

fn zoom(
    mut wheel: MessageReader<MouseWheel>,
    mut projection: Query<&mut Projection, With<MainCamera>>,
    mut goal: Local<Option<f32>>,
    time: Res<Time>,
) {
    let Ok(mut projection) = projection.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = &mut *projection else {
        return;
    };
    // Not known before the camera's first update.
    let h = ortho.area.height();
    if h <= 0.0 {
        return;
    }
    // Zoomed out at most to 12.5 tiles above and below the robber, and he starts
    // there unless BB_ZOOM says otherwise.
    let most = ortho.scale * 25.0 * TILE / h;
    let goal = goal.get_or_insert_with(|| {
        if crate::dev::env("BB_ZOOM").is_none() {
            ortho.scale = most;
        }
        ortho.scale
    });
    for e in wheel.read() {
        let notches = match e.unit {
            MouseScrollUnit::Line => e.y,
            MouseScrollUnit::Pixel => e.y / 40.0,
        };
        *goal *= 0.88f32.powf(notches);
    }
    *goal = goal.clamp(0.45f32.min(most), most);
    ortho.scale += (*goal - ortho.scale) * approach(14.0, time.delta_secs());
}
