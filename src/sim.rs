//! `--sim`: a bank alone to try the wraith in. The camera is free (WASD,
//! Shift faster, the wheel from close in out to the whole bank) and sees
//! everything; the wraith is always drawn, pale while it does not show
//! itself. H puts a ghost hunter at the pointer, carrying every capture
//! tool (quick slots: pistol, trap, lantern, coins), and gives him to
//! you (the one before stays where he stood, as bait); V switches between
//! the free camera and following him.

use crate::crew::{Look, Settings};
use crate::dev::Script;
use crate::inventory::{Inventory, Kind};
use crate::net::{Owner, Seed};
use crate::player::{Controls, Level, LocalPlayer, read_controls, spawn_hunter};
use crate::render::MainCamera;
use crate::smash::Bank;
use crate::wraith::{Mind, Wraith};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Pixels a second the free camera pans.
const PAN: f32 = 900.0;

/// The sim's camera: free of the hunter (it always is without one), and where.
#[derive(Resource, Default)]
pub struct Sim {
    pub free: bool,
    at: Vec2,
}

#[derive(Component)]
struct SimHud;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        if !crate::net::sim() {
            return;
        }
        app.init_resource::<Sim>()
            .add_systems(Startup, spawn_hud)
            .add_systems(PreUpdate, pan.after(read_controls))
            .add_systems(
                Update,
                (
                    hunter.run_if(resource_exists::<Level>.and_then(resource_exists::<Bank>)),
                    hud,
                ),
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn pan(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<Script>,
    time: Res<Time>,
    bank: Option<Res<Bank>>,
    seeds: Query<(), Added<Seed>>,
    me: Option<Single<&mut Controls, With<LocalPlayer>>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut sim: ResMut<Sim>,
    camera: Single<(&mut Transform, &Projection), With<MainCamera>>,
) {
    let (mut cam, projection) = camera.into_inner();
    // A new bank: over its middle, nobody in it yet.
    if let Some(bank) = bank.filter(|_| !seeds.is_empty()) {
        sim.at = bank.0.bounds().centre();
        sim.free = true;
    }
    if keys.just_pressed(KeyCode::KeyV) || script.pressed("FreeCam") {
        sim.free = !sim.free;
        sim.at = cam.translation.truncate();
    }
    if !sim.free && me.is_some() {
        return;
    }
    // His feet and hands are still while the camera is free.
    if let Some(mut c) = me {
        c.walk = None;
        c.sprint = false;
        c.roll = false;
        mouse.clear_just_pressed(MouseButton::Left);
    }
    let held = |k: KeyCode, alt: KeyCode, name: &str| {
        (keys.pressed(k) || keys.pressed(alt) || script.held(name)) as i32 as f32
    };
    let d = vec2(
        held(KeyCode::KeyD, KeyCode::ArrowRight, "D")
            - held(KeyCode::KeyA, KeyCode::ArrowLeft, "A"),
        held(KeyCode::KeyW, KeyCode::ArrowUp, "W") - held(KeyCode::KeyS, KeyCode::ArrowDown, "S"),
    );
    let fast = 1.0 + 2.0 * held(KeyCode::ShiftLeft, KeyCode::ShiftRight, "Shift");
    let Projection::Orthographic(o) = projection else {
        return;
    };
    sim.at += d.normalize_or_zero() * PAN * fast * o.scale * time.delta_secs();
    cam.translation = sim.at.extend(cam.translation.z);
}

/// H: a hunter at the pointer (off it, at `BB_SPAWN` or the bank's door),
/// played from now on.
#[allow(clippy::too_many_arguments)]
fn hunter(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<Script>,
    level: Res<Level>,
    bank: Res<Bank>,
    settings: Res<Settings>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut played: Query<(Entity, &mut Controls), With<LocalPlayer>>,
    mut sim: ResMut<Sim>,
    mut hunters: Local<usize>,
    mut commands: Commands,
) {
    if !(keys.just_pressed(KeyCode::KeyH) || script.pressed("Hunter")) {
        return;
    }
    let (camera, at) = *camera;
    let at = window
        .single()
        .ok()
        .and_then(Window::cursor_position)
        .and_then(|c| camera.viewport_to_world_2d(at, c).ok())
        .unwrap_or_else(|| crate::dev::tile("BB_SPAWN").unwrap_or(bank.0.spawn));
    if level.solids.solid_at(at) {
        return;
    }
    for (e, mut c) in &mut played {
        *c = Controls::default();
        commands.entity(e).remove::<(LocalPlayer, Owner)>();
    }
    *hunters += 1;
    let look = Look {
        name: format!("Hunter {}", *hunters),
        colour: settings.0.colour + *hunters - 1,
        ..settings.0.clone()
    };
    let mut gear = Inventory::default();
    for kind in [
        Kind::Trap,
        Kind::Lantern,
        Kind::Coffer,
        Kind::Thermometer,
        Kind::Emf,
        Kind::Infrared,
    ] {
        gear.add(kind, 1, None);
    }
    let find = |kind| gear.items.iter().position(|i| i.kind == kind);
    gear.quick = [Kind::Pistol, Kind::Trap, Kind::Lantern, Kind::Coin].map(find);
    let e = spawn_hunter(&mut commands, at, look);
    // Ours, so `net::mine` makes him this machine's.
    commands.entity(e).insert((Owner(0), gear));
    sim.free = false;
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        SimHud,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgba(0.75, 0.88, 1.0, 0.9)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            bottom: Val::Px(8.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.6)),
    ));
}

fn hud(
    sim: Res<Sim>,
    me: Option<Single<(), With<LocalPlayer>>>,
    wraiths: Query<(&Mind, &Wraith)>,
    mut text: Single<&mut Text, With<SimHud>>,
) {
    let camera = if sim.free || me.is_none() {
        "free camera: WASD pans, Shift faster"
    } else {
        "following your hunter"
    };
    let wraith = wraiths
        .iter()
        .next()
        .map_or("no bank".into(), |(m, w)| m.state(w));
    let line = format!(
        "SIMULATION\n\
         H hunter at the pointer | V free camera\n\
         {camera}\n\
         wheel zoom | F5 new bank\n\
         the wraith: {wraith}"
    );
    if text.0 != line {
        text.0 = line;
    }
}
