//! The ghost trap. In hand, a left click sets it on the floor in front of
//! the robber (`SetTrap`, to the server); E by a set one takes it back
//! (`TakeTrap`). The server keeps the traps (`Trap`), and the wraith is caught
//! in one (wraith.rs) once its name is spoken.

use crate::dev::Script;
use crate::inventory::{Inventory, Kind};
use crate::movement::TILE;
use crate::net::{Owner, sent_by};
use crate::paint;
use crate::player::{Dead, Level, Living, LocalPlayer, Player};
use crate::render::{WorldMaterials, WorldPart, things_material};
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, FromClient, Replicated};
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

/// Set this far ahead of him; taken back, or set at all, from this close.
const AHEAD: f32 = 0.9 * TILE;
const REACH: f32 = 1.2 * TILE;

#[derive(Component, Serialize, Deserialize)]
pub struct Trap {
    pub at: Vec2,
    /// Shut, with the wraith in it.
    pub sprung: bool,
}

/// Asked of the server: a trap set here, and one taken back from by him.
#[derive(Message, Serialize, Deserialize)]
pub struct SetTrap(pub Vec2);

#[derive(Message, Serialize, Deserialize)]
pub struct TakeTrap;

pub struct TrapPlugin;

impl Plugin for TrapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (set, take).distributive_run_if(
                resource_exists::<Level>.and_then(in_state(ClientState::Disconnected)),
            ),
        );
        if crate::drawn(app) {
            app.add_systems(Update, (place, pick_up, look));
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
        commands.spawn((WorldPart, Replicated, Trap { at, sprung: false }));
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

/// The trap in hand, a left click sets it.
fn place(
    mouse: Res<ButtonInput<MouseButton>>,
    script: Res<Script>,
    me: Single<(&Player, &mut Inventory), Living>,
    mut set: MessageWriter<SetTrap>,
) {
    let (p, mut inv) = me.into_inner();
    let click = mouse.just_pressed(MouseButton::Left) || script.pressed("SetTrap");
    let Some(id) = inv.held().filter(|_| inv.holds(Kind::Trap)) else {
        return;
    };
    if inv.open || !click {
        return;
    }
    let b = &p.body;
    set.write(SetTrap(b.pos + Vec2::from_angle(b.heading) * AHEAD));
    inv.use_one(id);
}

/// A set trap within his reach, if nothing else wants E: E takes it back.
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
    if (keys.just_pressed(KeyCode::KeyE) || script.pressed("Use")) && inv.add(Kind::Trap, 1, None) {
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
        commands.entity(e).insert((
            Mesh2d(meshes.add(paint::trap(t.sprung))),
            MeshMaterial2d(things_material(&world_materials)),
            Transform::from_translation(t.at.extend(0.7)),
            Visibility::default(),
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
