//! A guard: sent walking with T and a left click, and watching for the robber
//! with the robber's own sight, two tiles shorter where it is lit. Seeing
//! him, he gives chase; losing him, he runs to where he last saw him, which
//! a ghost of the robber marks. Hearing something, he goes to look, and a
//! "?" marks the spot. Come at from behind, E knocks him out for a while.

use crate::dev;
use crate::hearing::feet;
use crate::map::{Map, RoomKind, tile_centre, tile_of};
use crate::movement::{BODY_MARGIN, Body, MARCH_SPEED, STEP, Solids, Steer, TILE, approach};
use crate::paint;
use crate::player::{Controls, Player, Reach, Shown};
use crate::render::{WorldMaterial, WorldMaterials, WorldPart};
use crate::sight::{DARK, Sight, VIEW, smoothstep};
use crate::sound::{Clip, Noise};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

// Both also in world.wgsl.
const GUARD_VIEW: f32 = VIEW - 2.0;
/// Half his cone of sight, softened by 0.05 rad either side.
const CONE: f32 = FRAC_PI_4;
/// Chasing, he stops this close to the robber.
const CATCH: f32 = 2.0 * BODY_MARGIN;
/// Standing, he looks this far either way of where he stopped, and back, in so many seconds.
const LOOK_ABOUT: f32 = 10.0 * PI / 180.0;
const LOOK_PERIOD: f32 = 5.0;
/// Losing the robber, he goes on this many tiles past where he last saw him,
/// and there sweeps this far either way of where he went, once, before standing.
const FOLLOW_ON: i32 = 2;
const SEARCH: f32 = FRAC_PI_2;
/// Out of his sight, the robber's whereabouts stay known to him this long.
const TRACK: f32 = 1.0;
/// Knocked out, he lies this many seconds; the robber must be this close,
/// behind him.
const KNOCKED: f32 = 30.0;
const KNOCK_REACH: f32 = 1.3 * TILE;
/// His pistol's grip, held out in both hands, in his own frame.
const GRIP: f32 = 20.0;
/// His left shoulder in his own frame (the right is mirrored).
const SHOULDER: Vec2 = vec2(1.0, 14.5);

#[derive(Component)]
pub struct Guard {
    pub body: Body,
    /// Tile centres to walk through, the next last.
    path: Vec<Vec2>,
    pub sees: bool,
    /// Where the robber was and which way he was going when last known,
    /// until the guard gets there, and how long since he saw him.
    last_seen: Option<(Vec2, f32)>,
    unseen: f32,
    /// What he heard and goes to look at.
    heard: Option<Vec2>,
    /// Which way he faced as he stopped, how long he has stood, and how far
    /// his first look round goes.
    rest: f32,
    idle: f32,
    sweep: f32,
    /// How far his arm is out pointing at the robber, 0 to 1.
    point: f32,
    /// Seconds left knocked out, and how far down he is drawn, 0 to 1.
    out: f32,
    fall: f32,
    /// What he has picked up, by kind, how many.
    pub pockets: Vec<(crate::inventory::Kind, u32)>,
    /// Shots he can take; at none he is down for good.
    pub health: u32,
    /// How long his pistol has been out (0: holstered), and till his next shot.
    pub gun: f32,
    pub cool: f32,
}

#[derive(Component, Clone, Copy)]
enum Limb {
    Foot(i8),
    Hand(i8),
    Arm(i8),
    Torso,
    Head,
    /// His pistol, in his left hand while it is out.
    Gun,
}

/// The "Zz" over this guard while he is knocked out.
#[derive(Component)]
struct Asleep(Entity);

#[derive(Component)]
struct Alarm;

/// The robber where this guard last saw him, facing the way he went.
#[derive(Component)]
struct Ghost(Entity);

/// The "?" where this guard heard something.
#[derive(Component)]
struct Puzzled(Entity);

/// The tiles a body fits on, for paths.
#[derive(Resource)]
pub struct Walkable {
    w: i32,
    free: Vec<bool>,
}

/// T was pressed: the next left click sends the guard there.
#[derive(Resource, Default)]
pub struct Order(pub bool);

/// A guard sees the robber.
#[derive(Resource, Default)]
pub struct Spotted(pub bool);

/// A guard goes to see what he heard.
#[derive(Resource, Default)]
pub struct Heard(pub bool);

pub struct GuardPlugin;

impl Plugin for GuardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Order>()
            .init_resource::<Spotted>()
            .init_resource::<Heard>()
            .add_systems(
                PreUpdate,
                order
                    .after(bevy::input::InputSystems)
                    .run_if(resource_exists::<Walkable>),
            )
            .add_systems(FixedUpdate, walk.run_if(resource_exists::<Player>))
            .add_systems(
                Update,
                (knock, show, detect, alarm, ghost, puzzled, asleep)
                    .chain()
                    .run_if(resource_exists::<Player>.and_then(resource_exists::<Sight>)),
            );
    }
}

pub fn spawn_guard(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<WorldMaterial>,
    ghost: Handle<WorldMaterial>,
    map: &Map,
) {
    let free = map.free();
    let at = dev::tile("BB_GUARD").unwrap_or_else(|| post(map, &free));
    let p = paint::guard();
    let [foot, arm, glove] = [p.foot, p.arm, p.glove].map(|m| Mesh2d(meshes.add(m)));
    let mat = MeshMaterial2d(material);
    let guard = commands
        .spawn((
            WorldPart,
            Guard::new(at),
            Transform::from_translation(at.extend(9.0)),
            Visibility::default(),
        ))
        .with_children(|g| {
            g.spawn((
                Mesh2d(meshes.add(p.shadow)),
                mat.clone(),
                Transform::from_xyz(0.0, 0.0, -0.9),
            ));
            for side in [-1i8, 1] {
                let sd = side as f32;
                g.spawn((
                    Limb::Foot(side),
                    foot.clone(),
                    mat.clone(),
                    Transform::from_xyz(0.0, 8.0 * sd, -0.6),
                ));
                g.spawn((
                    Limb::Arm(side),
                    arm.clone(),
                    mat.clone(),
                    Transform::from_translation((SHOULDER * vec2(1.0, sd)).extend(0.3)),
                ));
                g.spawn((
                    Limb::Hand(side),
                    glove.clone(),
                    mat.clone(),
                    Transform::from_xyz(0.0, 0.0, 0.35),
                ));
            }
            g.spawn((
                Limb::Gun,
                Mesh2d(meshes.add(paint::pistol())),
                mat.clone(),
                Transform::from_xyz(0.0, 0.0, 0.34),
                Visibility::Hidden,
            ));
            g.spawn((
                Limb::Torso,
                Mesh2d(meshes.add(p.torso)),
                mat.clone(),
                Transform::from_xyz(-1.0, 0.0, 0.0),
            ));
            g.spawn((
                Limb::Head,
                Mesh2d(meshes.add(p.head)),
                mat.clone(),
                Transform::from_xyz(2.0, 0.0, 0.5),
            ));
        })
        .id();
    // The robber standing, without his tools.
    let r = paint::robber();
    let parts = [
        (r.foot.clone(), vec3(0.0, -8.0, -0.6), 0.0),
        (r.foot, vec3(0.0, 8.0, -0.6), 0.0),
        (r.bag, vec3(-13.0, 0.0, 0.2), 0.0),
        (r.torso, vec3(-1.0, 0.0, 0.0), 0.0),
        (r.arm.clone(), vec3(1.0, -14.0, 0.3), 0.42),
        (r.arm, vec3(1.0, 14.0, 0.3), -0.42),
        (r.head, vec3(2.0, 0.0, 0.5), 0.0),
    ];
    commands
        .spawn((
            WorldPart,
            Ghost(guard),
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|g| {
            for (mesh, at, turn) in parts {
                g.spawn((
                    Mesh2d(meshes.add(mesh)),
                    MeshMaterial2d(ghost.clone()),
                    Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
                ));
            }
        });
    commands.spawn((
        WorldPart,
        Asleep(guard),
        Mesh2d(meshes.add(paint::snore())),
        mat.clone(),
        Transform::from_xyz(0.0, 0.0, 29.0),
        Visibility::Hidden,
    ));
    commands.spawn((
        WorldPart,
        Puzzled(guard),
        Mesh2d(meshes.add(paint::question())),
        mat.clone(),
        Transform::from_xyz(0.0, 0.0, 29.0),
        Visibility::Hidden,
    ));
    commands.spawn((
        WorldPart,
        Alarm,
        Mesh2d(meshes.add(paint::alarm())),
        mat,
        Transform::from_xyz(0.0, 0.0, 30.0),
        Visibility::Hidden,
    ));
    commands.insert_resource(Walkable::new(map));
}

/// The free tile nearest the middle of the security room, else the lobby.
fn post(map: &Map, free: &[bool]) -> Vec2 {
    let room = [RoomKind::Security, RoomKind::Lobby]
        .iter()
        .find_map(|&k| map.rooms.iter().find(|r| r.kind == k));
    let Some(r) = room else {
        return map.spawn;
    };
    let mid = tile_centre((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    (r.y0..=r.y1)
        .flat_map(|y| (r.x0..=r.x1).map(move |x| (x, y)))
        .filter(|&(x, y)| free[map.idx(x, y)])
        .map(|(x, y)| tile_centre(x, y))
        .min_by(|a, b| a.distance_squared(mid).total_cmp(&b.distance_squared(mid)))
        .unwrap_or(map.spawn)
}

impl Walkable {
    pub fn new(map: &Map) -> Self {
        Walkable {
            w: map.w,
            free: map.free(),
        }
    }

    /// Tile centres from `from` to the reachable free tile nearest `to`, the
    /// next last. Diagonals only between two free sides.
    fn path(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let (w, h) = (self.w, self.free.len() as i32 / self.w);
        let ok =
            |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && self.free[(y * w + x) as usize];
        let (sx, sy) = tile_of(from);
        if sx < 0 || sy < 0 || sx >= w || sy >= h {
            return Vec::new();
        }
        let start = (sy * w + sx) as usize;
        let mut cost = vec![u32::MAX; self.free.len()];
        let mut prev = vec![start; self.free.len()];
        let mut heap = BinaryHeap::from([Reverse((0, start))]);
        cost[start] = 0;
        while let Some(Reverse((c, i))) = heap.pop() {
            if c > cost[i] {
                continue;
            }
            let (x, y) = (i as i32 % w, i as i32 / w);
            for (dx, dy) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let diagonal = dx != 0 && dy != 0;
                if !ok(x + dx, y + dy) || (diagonal && !(ok(x + dx, y) && ok(x, y + dy))) {
                    continue;
                }
                let n = ((y + dy) * w + x + dx) as usize;
                let nc = c + if diagonal { 7 } else { 5 };
                if nc < cost[n] {
                    cost[n] = nc;
                    prev[n] = i;
                    heap.push(Reverse((nc, n)));
                }
            }
        }
        let centre = |i: usize| tile_centre(i as i32 % w, i as i32 / w);
        let near = |i: usize| centre(i).distance_squared(to);
        let Some(mut i) = (0..cost.len())
            .filter(|&i| cost[i] < u32::MAX)
            .min_by(|&a, &b| near(a).total_cmp(&near(b)))
        else {
            return Vec::new();
        };
        let mut path = Vec::new();
        while i != start {
            path.push(centre(i));
            i = prev[i];
        }
        path
    }

    /// Whether the straight line from `a` to `b` keeps to free tiles.
    fn clear(&self, a: Vec2, b: Vec2) -> bool {
        let n = (a.distance(b) / (TILE / 4.0)).ceil().max(1.0);
        (1..=n as i32).all(|k| {
            let (x, y) = tile_of(a.lerp(b, k as f32 / n));
            let i = (y * self.w + x) as usize;
            x >= 0 && y >= 0 && x < self.w && i < self.free.len() && self.free[i]
        })
    }

    /// `path`, ending on `to` itself: a spot the robber stood on.
    fn path_to(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let mut path = self.path(from, to);
        match path.first() {
            Some(&end) if tile_of(end) == tile_of(to) => path[0] = to,
            _ => path.insert(0, to),
        }
        path
    }
}

impl Guard {
    fn new(at: Vec2) -> Self {
        Guard {
            body: Body::new(at, -FRAC_PI_2),
            path: Vec::new(),
            sees: false,
            last_seen: None,
            unseen: 0.0,
            heard: None,
            rest: -FRAC_PI_2,
            idle: 0.0,
            sweep: LOOK_ABOUT,
            point: 0.0,
            out: 0.0,
            fall: 0.0,
            pockets: Vec::new(),
            health: 3,
            gun: 0.0,
            cool: 0.0,
        }
    }

    /// Hit by a shot: down for good at the last.
    pub fn hit(&mut self) {
        self.health = self.health.saturating_sub(1);
        if self.health == 0 {
            self.out = f32::INFINITY;
            self.gun = 0.0;
            self.path.clear();
        }
    }

    /// Where his shot leaves his pistol, out in his left hand.
    pub fn muzzle(&self) -> Vec2 {
        self.body.pos + Vec2::from_angle(self.body.heading) * (GRIP + 16.0)
    }

    pub fn awake(&self) -> bool {
        self.out <= 0.0
    }

    pub fn pocket(&mut self, kind: crate::inventory::Kind, n: u32) {
        match self.pockets.iter_mut().find(|p| p.0 == kind) {
            Some(p) => p.1 += n,
            None => self.pockets.push((kind, n)),
        }
    }

    /// Where a glove is in his frame: hanging under his shoulder (the right on
    /// his holster), swung against his feet; seeing the robber, the left
    /// points at him; his pistol out, both on it.
    fn hand(&self, side: i8) -> Vec2 {
        let sd = side as f32;
        let b = &self.body;
        let s = b.stride.sin() * (b.speed / MARCH_SPEED).clamp(0.0, 1.0);
        let hang = vec2(3.0 - 5.0 * s * sd, 18.5 * sd);
        let up = if side > 0 {
            hang.lerp(vec2(17.0, 7.0), self.point)
        } else {
            hang
        };
        // His pistol out, both hands on it at arm's length ahead.
        let held = if side < 0 {
            vec2(GRIP, -1.5)
        } else {
            vec2(GRIP - 2.0, 3.0)
        };
        let up = up.lerp(held, (self.gun / 0.25).min(1.0));
        // Down, his arms lie flung out.
        up.lerp(vec2(7.0, 25.0 * sd), self.fall)
    }

    /// Where he looks from and which way (cos, sin), between fixed steps.
    pub fn view(&self, between: f32) -> Vec4 {
        if self.out > 0.0 {
            return Vec4::ZERO;
        }
        let at = self.body.was.lerp(self.body.pos, between);
        at.extend(self.body.heading.cos())
            .extend(self.body.heading.sin())
    }

    fn in_cone(&self, at: Vec2) -> f32 {
        let ahead = Vec2::from_angle(self.body.heading);
        let c = (at - self.body.pos).normalize_or_zero().dot(ahead);
        smoothstep((CONE + 0.05).cos(), (CONE - 0.05).cos(), c)
    }

    /// One fixed step: on to a corner's tile from a third of a tile out, all
    /// the way to the last.
    fn step(&mut self, solids: &Solids) {
        let down = if self.out > 0.0 { 1.0 } else { 0.0 };
        self.fall += (down - self.fall) * approach(10.0, STEP);
        if self.out > 0.0 {
            self.out -= STEP;
            self.body.set_steer(Steer {
                aim: self.body.heading,
                ..default()
            });
            self.body.update(STEP, solids);
            return;
        }
        while let Some(&next) = self.path.last() {
            let close = if self.path.len() > 1 { TILE / 3.0 } else { 2.0 };
            if next.distance(self.body.pos) > close {
                break;
            }
            self.path.pop();
        }
        let walk = self.path.last().map(|&p| (p - self.body.pos).to_angle());
        // Seeing the robber he keeps his eyes on him, else on the way ahead.
        let look = self
            .last_seen
            .filter(|_| self.sees)
            .map(|(p, _)| (p - self.body.pos).to_angle());
        let aim = if let Some(a) = look.or(walk) {
            self.idle = 0.0;
            a
        } else {
            if self.idle == 0.0 {
                // There and nobody: he searches about the way the robber
                // went, or about where he heard something.
                let lost = (self.unseen >= TRACK)
                    .then(|| self.last_seen.take())
                    .flatten();
                let heard = self.heard.take();
                self.rest = lost.map_or(self.body.heading, |l| l.1);
                self.sweep = if lost.is_some() || heard.is_some() {
                    SEARCH
                } else {
                    LOOK_ABOUT
                };
            }
            self.idle += STEP;
            let sweep = if self.idle < LOOK_PERIOD {
                self.sweep
            } else {
                LOOK_ABOUT
            };
            self.rest + sweep * (self.idle * TAU / LOOK_PERIOD).sin()
        };
        self.body.set_steer(Steer {
            walk,
            aim,
            ..default()
        });
        self.body.update(STEP, solids);
        let goal = if self.sees { 1.0 } else { 0.0 };
        self.point += (goal - self.point) * approach(12.0, STEP);
    }
}

#[allow(clippy::too_many_arguments)]
fn order(
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform)>,
    walkable: Res<Walkable>,
    inventory: Res<crate::inventory::Inventory>,
    mut armed: ResMut<Order>,
    mut guards: Query<&mut Guard>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        armed.0 = !armed.0;
    }
    if !armed.0 || inventory.open || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    // The click is the order's, not the crowbar's.
    mouse.clear_just_pressed(MouseButton::Left);
    let (Ok(window), Ok((camera, at))) = (window.single(), camera.single()) else {
        return;
    };
    let Some(to) = window
        .cursor_position()
        .and_then(|c| camera.viewport_to_world_2d(at, c).ok())
    else {
        return;
    };
    armed.0 = false;
    for mut g in &mut guards {
        g.path = walkable.path(g.body.pos, to);
        g.last_seen = None;
    }
}

fn walk(player: Res<Player>, mut guards: Query<&mut Guard>) {
    for mut g in &mut guards {
        g.step(&player.solids);
    }
}

fn show(
    fixed: Res<Time<Fixed>>,
    mut roots: Query<(&Guard, &mut Transform)>,
    mut limbs: Query<(&Limb, &ChildOf, &mut Transform, &mut Visibility), Without<Guard>>,
) {
    for (g, mut t) in &mut roots {
        let b = &g.body;
        t.translation = b.was.lerp(b.pos, fixed.overstep_fraction()).extend(9.0);
        t.rotation = Quat::from_rotation_z(b.heading);
    }
    for (limb, parent, mut t, mut vis) in &mut limbs {
        let Ok((g, _)) = roots.get(parent.parent()) else {
            continue;
        };
        let b = &g.body;
        let s = b.stride.sin() * (b.speed / MARCH_SPEED).clamp(0.0, 1.0);
        // Down, he lies on his back: head past his shoulders, legs out behind.
        let k = g.fall;
        match *limb {
            Limb::Foot(side) => {
                let sd = side as f32;
                let at = vec2(s * sd * 9.5, 8.0 * sd).lerp(vec2(-25.0, 7.0 * sd), k);
                t.translation = at.extend(t.translation.z);
            }
            Limb::Torso => t.scale.x = 1.0 + 0.2 * k,
            Limb::Head => t.translation.x = 2.0 + 15.0 * k,
            Limb::Gun => {
                *vis = if g.gun > 0.0 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                t.translation = vec2(GRIP, 0.0).extend(t.translation.z);
            }
            Limb::Hand(side) => {
                let at = g.hand(side);
                t.translation = at.extend(t.translation.z);
            }
            // The sleeve, from the shoulder to the glove.
            Limb::Arm(side) => {
                let d = g.hand(side) - SHOULDER * vec2(1.0, side as f32);
                t.rotation = Quat::from_rotation_z(d.to_angle());
                t.scale.x = (d.length() - 1.5).max(0.0) / 9.0;
            }
        }
    }
}

/// Sight goes both ways, so it is traced from the robber as the shader
/// traces his: crouched, the cover that hides a guard from him hides him.
/// His flashlight on, he is lit, as world.wgsl's `watched` finds it.
#[allow(clippy::too_many_arguments)]
pub fn detect(
    player: Res<Player>,
    controls: Res<Controls>,
    sight: Res<Sight>,
    walkable: Res<Walkable>,
    mut guards: Query<&mut Guard>,
    world_materials: Res<WorldMaterials>,
    images: Res<Assets<Image>>,
    time: Res<Time>,
    mut noises: MessageReader<Noise>,
    mut spotted: ResMut<Spotted>,
    mut heard: ResMut<Heard>,
) {
    let Some(doors) = images
        .get(&world_materials.doors)
        .and_then(|i| i.data.as_deref())
    else {
        return;
    };
    let b = &player.body;
    let reach = if controls.torch || sight.lit_at(b.pos) {
        GUARD_VIEW
    } else {
        DARK
    };
    spotted.0 = false;
    heard.0 = false;
    let dt = time.delta_secs();
    // What there is to hear: noises made this frame, and his feet.
    let mut sounds: Vec<(Vec2, f32)> = noises
        .read()
        .filter(|n| n.loud > 0.0)
        .map(|n| (n.at, n.loud))
        .collect();
    if feet(b) > 0.0 {
        sounds.push((b.pos, feet(b)));
    }
    // How far each carries, tile by tile.
    let carried: Vec<(Vec2, Vec<f32>)> = sounds
        .iter()
        .map(|&(at, loud)| (at, sight.carry(at, loud, doors)))
        .collect();
    // In a thicket he is hidden, from a guard who did not see him go in,
    // unless that guard is in it with him.
    let den = player.hideout();
    for mut g in &mut guards {
        if g.out > 0.0 {
            continue;
        }
        let hidden = den.is_some_and(|d| !d.contains(g.body.pos));
        g.sees = (g.sees || !hidden)
            && g.in_cone(b.pos) * sight.seen(b.pos, g.body.pos, reach, b.is_crouching(), doors)
                > 0.5;
        spotted.0 |= g.sees;
        g.unseen = if g.sees { 0.0 } else { g.unseen + dt };
        // Seen, or just out of sight: he goes for where the robber is.
        if g.sees || (g.last_seen.is_some() && g.unseen < TRACK) {
            let going = if b.speed > 1.0 { b.intent } else { b.heading };
            g.last_seen = Some((b.pos, going));
            g.path = if g.sees && g.body.pos.distance(b.pos) < CATCH {
                Vec::new()
            } else {
                walkable.path_to(g.body.pos, b.pos)
            };
        } else if g.unseen - dt < TRACK
            && let Some((at, going)) = g.last_seen
        {
            let on = (1..=FOLLOW_ON)
                .rev()
                .map(|k| at + Vec2::from_angle(going) * (k as f32 * TILE))
                .find(|&p| walkable.clear(at, p))
                .unwrap_or(at);
            g.path = walkable.path_to(g.body.pos, on);
        }
        // Not after the robber, he goes to look at the nearest thing he hears.
        let after = g.sees || (g.last_seen.is_some() && g.unseen < TRACK);
        let near = |at: &Vec2| at.distance(g.body.pos);
        if !after
            && let Some(ear) = sight.index(g.body.pos)
            && let Some(at) = carried
                .iter()
                .filter(|(_, left)| left[ear] > 0.0)
                .map(|&(at, _)| at)
                .min_by(|a, b| near(a).total_cmp(&near(b)))
        {
            g.last_seen = None;
            g.heard = Some(at);
            g.path = walkable.path_to(g.body.pos, at);
        }
        if g.sees {
            g.heard = None;
        }
        heard.0 |= g.heard.is_some();
    }
}

fn alarm(
    spotted: Res<Spotted>,
    shown: Res<Shown>,
    time: Res<Time>,
    mut since: Local<f32>,
    mut mark: Query<(&mut Transform, &mut Visibility), With<Alarm>>,
) {
    let Ok((mut t, mut vis)) = mark.single_mut() else {
        return;
    };
    *since = if spotted.0 {
        *since + time.delta_secs()
    } else {
        0.0
    };
    *vis = if spotted.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    // It pops up, a little too big at first.
    let pop = 1.0 - (-12.0 * *since).exp() * (20.0 * *since).cos();
    t.translation = (shown.pos + vec2(0.0, 36.0)).extend(30.0);
    t.scale = Vec3::splat(pop);
}

fn ghost(guards: Query<&Guard>, mut ghosts: Query<(&Ghost, &mut Transform, &mut Visibility)>) {
    for (ghost, mut t, mut vis) in &mut ghosts {
        let last = guards
            .get(ghost.0)
            .ok()
            .filter(|g| g.unseen >= TRACK)
            .and_then(|g| g.last_seen);
        *vis = if last.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some((at, heading)) = last {
            t.translation = at.extend(8.0);
            t.rotation = Quat::from_rotation_z(heading);
        }
    }
}

/// E, right behind a guard who does not see him: the robber knocks him out
/// (the prompt on the guard's far side from him).
#[allow(clippy::too_many_arguments)]
pub fn knock(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<crate::dev::Script>,
    inventory: Res<crate::inventory::Inventory>,
    player: Res<Player>,
    mut reach: ResMut<Reach>,
    mut prompt: ResMut<crate::prompt::Prompt>,
    mut guards: Query<&mut Guard>,
    mut noises: MessageWriter<Noise>,
) {
    let b = &player.body;
    let behind = |g: &Guard| {
        let off = b.pos - g.body.pos;
        g.out <= 0.0
            && !g.sees
            && off.length() < KNOCK_REACH
            && off
                .normalize_or_zero()
                .dot(Vec2::from_angle(g.body.heading))
                < -0.2
    };
    let free = !inventory.open && !b.is_rolling();
    let Some(g) = guards.iter().find(|g| free && behind(g)) else {
        return;
    };
    let at = g.body.pos + (g.body.pos - b.pos).normalize_or_zero() * 40.0;
    prompt.0 = Some((at, "Knock out"));
    if !(keys.just_pressed(KeyCode::KeyE) || script.pressed("Use")) {
        return;
    }
    if let Some(mut g) = guards.iter_mut().find(|g| behind(g)) {
        reach.0 = Some((g.body.pos, 0.0));
        noises.write(Noise {
            clip: Some(Clip::Knockout),
            at: g.body.pos,
            loud: 0.0,
        });
        g.out = KNOCKED;
        g.path.clear();
        g.last_seen = None;
        g.heard = None;
    }
}

fn asleep(
    time: Res<Time>,
    guards: Query<&Guard>,
    mut marks: Query<(&Asleep, &mut Transform, &mut Visibility)>,
) {
    for (mark, mut t, mut vis) in &mut marks {
        // Asleep, not down for good.
        let out = guards
            .get(mark.0)
            .ok()
            .filter(|g| g.out > 0.0 && g.out.is_finite());
        *vis = if out.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(g) = out {
            let drift = (time.elapsed_secs() * 1.5).sin();
            t.translation = (g.body.pos + vec2(10.0 + 3.0 * drift, 30.0)).extend(29.0);
        }
    }
}

fn puzzled(
    time: Res<Time>,
    guards: Query<&Guard>,
    mut marks: Query<(&Puzzled, &mut Transform, &mut Visibility)>,
) {
    for (mark, mut t, mut vis) in &mut marks {
        let at = guards.get(mark.0).ok().and_then(|g| g.heard);
        *vis = if at.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(at) = at {
            let bob = 3.0 * (time.elapsed_secs() * 4.0).sin();
            t.translation = (at + vec2(0.0, bob)).extend(29.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::generate;

    #[test]
    fn a_guard_walks_from_the_street_into_the_vault() {
        for seed in 0..8 {
            let map = generate(seed);
            let v = *map
                .rooms
                .iter()
                .find(|r| r.kind == RoomKind::Vault)
                .unwrap();
            let walkable = Walkable {
                w: map.w,
                free: map.free(),
            };
            let path = walkable.path(map.spawn, tile_centre(v.x0, v.y0));
            let end = path[0];
            let (x, y) = tile_of(end);
            assert!((v.x0..=v.x1).contains(&x) && (v.y0..=v.y1).contains(&y));
            let solids = Solids::new(map.bounds(), map.solids());
            let mut g = Guard::new(map.spawn);
            g.path = path;
            for _ in 0..60 * 120 {
                g.step(&solids);
            }
            let at = tile_of(g.body.pos);
            assert!(
                g.body.pos.distance(end) < 3.0,
                "seed {seed}: stuck at {at:?}"
            );
        }
    }

    #[test]
    fn knocked_out_he_lies_still_then_gets_up() {
        let map = generate(6);
        let solids = Solids::new(map.bounds(), map.solids());
        let mut g = Guard::new(map.spawn);
        g.path = Walkable::new(&map).path(map.spawn, map.spawn + vec2(4.0 * TILE, 0.0));
        g.out = KNOCKED;
        for _ in 0..(60.0 * (KNOCKED - 0.5)) as usize {
            g.step(&solids);
        }
        assert_eq!(g.body.pos, map.spawn);
        assert_eq!(g.view(0.0), Vec4::ZERO, "no cone while he is out");
        for _ in 0..120 {
            g.step(&solids);
        }
        assert!(g.out <= 0.0 && g.body.pos != map.spawn, "up and on his way");
    }

    #[test]
    fn noises_go_round_corners_and_lose_reach_to_walls() {
        use crate::hearing::{SPRINT_LOUD, WALK_LOUD};
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
