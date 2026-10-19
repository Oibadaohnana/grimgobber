//! The ghost trap. In hand, a left click sets it on the floor in front of
//! the robber (`SetTrap`, to the server), and it turns into its trigger in
//! his hand; E by a set one takes it back (`TakeTrap`), the trigger with it.
//! A left click with the trigger fires every set trap (`Pull`): a blue net
//! springs out `CATCHES` round each for `NET_TIME`, and a banshee in it
//! while it is out is caught, which wins the round; any other kind is
//! enraged. A hunting ghost crosses the net's reach in under half a
//! second, so the net is live all the while it is out, not just as pulled.

use crate::dev::Script;
use crate::inventory::{Inventory, Kind};
use crate::movement::TILE;
use crate::net::{Owner, sent_by};
use crate::paint;
use crate::player::{Dead, Level, Living, LocalPlayer, Player};
use crate::render::{WorldMaterials, WorldPart, things_material};
use crate::wraith::{Ghost, Mind, Wraith};
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, FromClient, Replicated};
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

/// Set this far ahead of him; taken back, or set at all, from this close.
const AHEAD: f32 = 0.9 * TILE;
const REACH: f32 = 1.2 * TILE;
/// The net takes the wraith this close to the trap; it is out so long, and
/// the trap fires again only after it.
const CATCHES: f32 = 1.5 * TILE;
const NET_TIME: f32 = 0.8;

#[derive(Component, Default, Serialize, Deserialize)]
pub struct Trap {
    pub at: Vec2,
    /// Shut, with the wraith in it.
    pub sprung: bool,
    /// How often it has fired: a change shows the net.
    pub shots: u8,
    /// On the server: when it last fired, and by whom while its net is out
    /// and has caught nothing yet.
    #[serde(skip)]
    fired: f32,
    #[serde(skip)]
    live: Option<Entity>,
}

/// Asked of the server: a trap set here, one taken back by him, the traps fired.
#[derive(Message, Serialize, Deserialize)]
pub struct SetTrap(pub Vec2);

#[derive(Message, Serialize, Deserialize)]
pub struct TakeTrap;

#[derive(Message, Serialize, Deserialize)]
pub struct Pull;

/// The net's picture: the shots shown, seconds since the last.
#[derive(Component)]
struct Net {
    shots: u8,
    t: f32,
}

pub struct TrapPlugin;

impl Plugin for TrapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (set, take, fire, catch).chain().distributive_run_if(
                resource_exists::<Level>.and_then(in_state(ClientState::Disconnected)),
            ),
        );
        if crate::drawn(app) {
            // Pulled before set: the click that sets it doesn't fire it.
            app.add_systems(Update, ((pull, place).chain(), pick_up, look, net));
        }
    }
}

type Asker<'a> = (Entity, &'a Player, Option<&'a Owner>, Has<LocalPlayer>);

fn set(
    mut asks: MessageReader<FromClient<SetTrap>>,
    ids: Query<&NetworkId>,
    level: Res<Level>,
    robbers: Query<Asker, Without<Dead>>,
    mut commands: Commands,
) {
    for FromClient { client_id, message } in asks.read() {
        let Some((_, p, ..)) = robbers.iter().find(|r| sent_by(*client_id, &ids, r.2, r.3)) else {
            continue;
        };
        let pos = p.body.pos;
        if pos.distance(message.0) > REACH {
            continue;
        }
        // Not into a wall or a cabinet: at his feet then.
        let at = if level.solids.solid_at(message.0) {
            pos
        } else {
            message.0
        };
        commands.spawn((WorldPart, Replicated, Trap { at, ..default() }));
    }
}

fn take(
    mut asks: MessageReader<FromClient<TakeTrap>>,
    ids: Query<&NetworkId>,
    robbers: Query<Asker, Without<Dead>>,
    traps: Query<(Entity, &Trap)>,
    mut commands: Commands,
) {
    for FromClient { client_id, .. } in asks.read() {
        let Some((_, p, ..)) = robbers.iter().find(|r| sent_by(*client_id, &ids, r.2, r.3)) else {
            continue;
        };
        if let Some((e, _)) = traps
            .iter()
            .find(|(_, t)| !t.sprung && t.at.distance(p.body.pos) < REACH)
        {
            commands.entity(e).despawn();
        }
    }
}

/// A trigger pulled: every set trap that is ready fires its net.
fn fire(
    time: Res<Time>,
    mut asks: MessageReader<FromClient<Pull>>,
    ids: Query<&NetworkId>,
    robbers: Query<Asker, Without<Dead>>,
    mut traps: Query<&mut Trap>,
) {
    let now = time.elapsed_secs();
    for FromClient { client_id, .. } in asks.read() {
        let Some((e, ..)) = robbers.iter().find(|r| sent_by(*client_id, &ids, r.2, r.3)) else {
            continue;
        };
        for mut trap in &mut traps {
            if trap.sprung || (trap.shots > 0 && now - trap.fired < NET_TIME) {
                continue;
            }
            trap.shots = trap.shots.wrapping_add(1);
            trap.fired = now;
            trap.live = Some(e);
        }
    }
}

/// A ghost in a net while it is out: a banshee is caught; any other kind,
/// once a net, turns on whoever pulled.
fn catch(
    time: Res<Time>,
    robbers: Query<&Player>,
    mut traps: Query<&mut Trap>,
    mut wraiths: Query<(&mut Mind, &mut Wraith)>,
) {
    let now = time.elapsed_secs();
    for mut trap in &mut traps {
        let Some(by) = trap.live else {
            continue;
        };
        if now - trap.fired >= NET_TIME {
            trap.live = None;
            continue;
        }
        for (mut m, mut w) in &mut wraiths {
            if w.caught || w.pos.distance(trap.at) >= CATCHES {
                continue;
            }
            trap.live = None;
            if m.ghost != Ghost::Banshee {
                let at = robbers.get(by).map_or(trap.at, |p| p.body.pos);
                m.enrage(&mut w, by, at);
                continue;
            }
            info!("the banshee is caught");
            trap.sprung = true;
            w.caught = true;
            w.pos = trap.at;
            w.flicker = None;
        }
    }
}

fn click(mouse: &ButtonInput<MouseButton>, script: &Script, name: &str) -> bool {
    mouse.just_pressed(MouseButton::Left) || script.pressed(name)
}

/// The trigger in hand, a left click fires the traps.
fn pull(
    mouse: Res<ButtonInput<MouseButton>>,
    script: Res<Script>,
    me: Single<&Inventory, Living>,
    mut pull: MessageWriter<Pull>,
) {
    if !me.open && me.holds(Kind::Trigger) && click(&mouse, &script, "Pull") {
        pull.write(Pull);
    }
}

/// The trap in hand, a left click sets it, its trigger left in his hand.
fn place(
    mouse: Res<ButtonInput<MouseButton>>,
    script: Res<Script>,
    me: Single<(&Player, &mut Inventory), Living>,
    mut set: MessageWriter<SetTrap>,
) {
    let (p, mut inv) = me.into_inner();
    let Some(id) = inv.held().filter(|_| inv.holds(Kind::Trap)) else {
        return;
    };
    if inv.open || !click(&mouse, &script, "SetTrap") {
        return;
    }
    let b = &p.body;
    set.write(SetTrap(b.pos + Vec2::from_angle(b.heading) * AHEAD));
    inv.change(id, Kind::Trigger);
}

/// A set trap within his reach, if nothing else wants E: E takes it back,
/// his trigger turning into it again.
fn pick_up(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<Script>,
    me: Single<(&Player, &mut Inventory), Living>,
    traps: Query<&Trap>,
    mut prompt: ResMut<crate::prompt::Prompt>,
    mut take: MessageWriter<TakeTrap>,
) {
    let (player, mut inv) = me.into_inner();
    if prompt.0.is_some() || inv.open {
        return;
    }
    let pos = player.body.pos;
    let Some(t) = traps
        .iter()
        .find(|t| !t.sprung && t.at.distance(pos) < REACH)
    else {
        return;
    };
    prompt.0 = Some((t.at + vec2(0.0, 30.0), "Take the trap"));
    if !(keys.just_pressed(KeyCode::KeyE) || script.pressed("Use")) {
        return;
    }
    let back = match inv.items.iter().position(|i| i.kind == Kind::Trigger) {
        Some(id) => inv.change(id, Kind::Trap),
        None => inv.add(Kind::Trap, 1, None),
    };
    if back {
        take.write(TakeTrap);
    }
}

pub fn dress(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    traps: Query<(Entity, &Trap), Added<Trap>>,
) {
    for (e, t) in &traps {
        commands
            .entity(e)
            .insert((
                Mesh2d(meshes.add(paint::trap(t.sprung))),
                MeshMaterial2d(things_material(&world_materials)),
                Transform::from_translation(t.at.extend(0.7)),
                Visibility::default(),
            ))
            .with_child((
                Net {
                    shots: t.shots,
                    t: NET_TIME,
                },
                Mesh2d(meshes.add(paint::net(0.0, 0.0))),
                MeshMaterial2d(things_material(&world_materials)),
                Transform::from_xyz(0.0, 0.0, 9.0),
                Visibility::Hidden,
            ));
    }
}

/// Sprung, it shuts, the wraith glowing in it.
fn look(mut meshes: ResMut<Assets<Mesh>>, traps: Query<(&Trap, &Mesh2d), Changed<Trap>>) {
    for (t, mesh) in &traps {
        if let Some(mut m) = meshes.get_mut(&mesh.0) {
            *m = paint::trap(t.sprung);
        }
    }
}

/// Fired, the net flies out to its reach, then draws shut on the trap.
fn net(
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    traps: Query<&Trap>,
    mut nets: Query<(&ChildOf, &mut Net, &Mesh2d, &mut Visibility)>,
) {
    for (parent, mut net, mesh, mut vis) in &mut nets {
        let Ok(trap) = traps.get(parent.parent()) else {
            continue;
        };
        if trap.shots != net.shots {
            net.shots = trap.shots;
            net.t = 0.0;
        }
        let out = net.t < NET_TIME;
        let want = if out {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
        if !out {
            continue;
        }
        net.t += time.delta_secs();
        let k = (net.t / NET_TIME).min(1.0);
        // Out fast, eased, held a moment, then drawn in.
        let r = if k < 0.35 {
            1.0 - (1.0 - k / 0.35).powi(3)
        } else {
            1.0 - ((k - 0.5) / 0.5).clamp(0.0, 1.0).powi(2)
        };
        if let Some(mut m) = meshes.get_mut(&mesh.0) {
            *m = paint::net(r * CATCHES, 1.0 - 0.6 * k);
        }
    }
}
