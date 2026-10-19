mod crew;
mod dev;
mod door;
mod doors;
mod guns;
mod hearing;
mod inventory;
mod inventory_ui;
mod map;
mod menu;
mod movement;
mod net;
mod paint;
mod paths;
mod player;
mod prompt;
mod render;
mod rng;
mod sanity;
mod sight;
mod smash;
mod sound;
mod talk;
mod throw;
mod trap;
mod wraith;

use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::log::LogPlugin;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::window::{PresentMode, WindowMode, WindowResolution};
use bevy_replicon::prelude::{Remote, Replicated};
use std::time::Duration;

use crate::player::{Controls, Level, LocalPlayer, Player, PlayerPlugin, spawn_robber};
use crate::render::{MainCamera, RenderPlugin, WorldMaterial, WorldMaterials, WorldPart};

/// The bank to build next, set by the rounds (crew.rs), F5, or the server's seed.
#[derive(Resource)]
pub struct NewMap(pub Option<u64>);

#[derive(Resource)]
struct Current {
    seed: u64,
    rooms: usize,
    props: usize,
    lamps: usize,
}

#[derive(Component)]
struct Hud;

/// A headless server: nobody plays on it, nothing is drawn.
#[derive(Resource)]
pub struct Headless;

/// Whether this app draws what it runs, which a server does not.
pub fn drawn(app: &App) -> bool {
    app.is_plugin_added::<WindowPlugin>()
}

fn main() {
    if let Some(port) = net::serve() {
        door::serve(port);
    }
    let seed = dev::env("BB_SEED")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(clock_seed);
    let role = net::role();
    let mut app = App::new();
    if let net::Role::Server(_) = role {
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                movement::STEP as f64,
            ))),
            LogPlugin::default(),
            StatesPlugin,
        ))
        .insert_resource(Headless);
    } else {
        let (w, h) = dev::window_size();
        let smoke = dev::smoke_frames().is_some();
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Grimgobber".into(),
                resolution: WindowResolution::new(w, h),
                present_mode: if smoke {
                    PresentMode::AutoNoVsync
                } else {
                    PresentMode::AutoVsync
                },
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins((
            RenderPlugin,
            sound::SoundPlugin,
            hearing::HearingPlugin,
            inventory_ui::InventoryUiPlugin,
            talk::TalkPlugin,
            menu::MenuPlugin,
            smash::SmashPlugin,
            throw::ThrowPlugin,
            prompt::PromptPlugin,
            guns::GunsPlugin,
        ))
        .insert_resource(ClearColor(Color::srgb(0.01, 0.012, 0.02)))
        .init_resource::<WorldMaterials>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                keys.before(build_map),
                (
                    draw_map.run_if(resource_exists_and_changed::<Current>),
                    player::dress,
                    doors::dress,
                    wraith::dress,
                    wraith::dress_notes,
                    trap::dress,
                    hud,
                )
                    .chain()
                    .after(build_map),
            ),
        );
    }
    app.add_message::<sound::Noise>()
        .add_plugins((
            net::NetPlugin,
            PlayerPlugin,
            doors::DoorsPlugin,
            wraith::WraithPlugin,
            sanity::SanityPlugin,
            trap::TrapPlugin,
            crew::CrewPlugin,
            dev::DevPlugin,
        ))
        .insert_resource(NewMap(None))
        .insert_resource(match role {
            net::Role::Client(_) => net::Mode::Online(None),
            // A smoke run plays alone at once, unless it is to try the title.
            _ if dev::smoke_frames().is_some()
                && ["BB_TITLE", "BB_JOIN", "BB_HOST"]
                    .iter()
                    .all(|v| dev::env(v).is_none()) =>
            {
                net::Mode::Alone
            }
            _ => net::Mode::Title,
        })
        .insert_resource(crew::NextSeed(seed))
        .add_systems(Update, (served, clear, build_map).chain())
        .run();
}

fn clock_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() % 100_000)
        .unwrap_or(1)
}

fn setup(mut commands: Commands) {
    let scale = dev::env("BB_ZOOM")
        .and_then(|z| z.parse().ok())
        .unwrap_or(0.7f32);
    commands.spawn((
        MainCamera,
        Camera2d,
        Hdr,
        Tonemapping::None,
        Bloom {
            intensity: 0.12,
            ..Bloom::NATURAL
        },
        Projection::Orthographic(OrthographicProjection {
            scale,
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgba(0.85, 0.85, 0.8, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            top: Val::Px(8.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.6)),
    ));
}

fn keys(
    input: Res<ButtonInput<KeyCode>>,
    script: Res<dev::Script>,
    current: Option<Res<Current>>,
    joined: Option<Res<net::Joined>>,
    mut new_map: ResMut<NewMap>,
    mut window: Query<&mut Window>,
) {
    if (input.just_pressed(KeyCode::F5) || script.pressed("NewBank")) && joined.is_none() {
        let next = current.map_or(1, |c| {
            c.seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
                % 100_000
        });
        new_map.0 = Some(next);
    }
    if input.just_pressed(KeyCode::F11)
        && let Ok(mut w) = window.single_mut()
    {
        w.mode = match w.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
}

/// A client builds the bank the server's seed gives.
fn served(seeds: Query<&net::Seed, (Added<net::Seed>, With<Remote>)>, mut new_map: ResMut<NewMap>) {
    if let Some(seed) = seeds.iter().last() {
        new_map.0 = Some(seed.0);
    }
}

/// The round over (its seed gone), what was built for it goes too.
fn clear(
    current: Option<Res<Current>>,
    seeds: Query<(), With<net::Seed>>,
    new_map: Res<NewMap>,
    parts: Query<Entity, With<WorldPart>>,
    mut commands: Commands,
) {
    if current.is_none() || !seeds.is_empty() || new_map.0.is_some() {
        return;
    }
    for e in &parts {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<Current>();
    commands.remove_resource::<Level>();
    commands.remove_resource::<sight::Sight>();
    commands.remove_resource::<doors::Shut>();
    commands.remove_resource::<smash::Bank>();
    commands.remove_resource::<paths::Walkable>();
}

/// A robber for each crewmate, in his look; who is in it comes from the
/// server when playing on one.
fn build_map(
    mut commands: Commands,
    mut new_map: ResMut<NewMap>,
    joined: Option<Res<net::Joined>>,
    old: Query<Entity, With<WorldPart>>,
    mut crew: Query<(&mut crew::Crewmate, &crew::Look)>,
) {
    let Some(seed) = new_map.0.take() else {
        return;
    };
    for e in &old {
        commands.entity(e).despawn();
    }
    let map = map::generate(seed);
    commands.insert_resource(sight::Sight::new(&map));
    commands.insert_resource(doors::Shut::new(&map));
    commands.insert_resource(Level::new(&map));
    if joined.is_none() {
        commands.spawn((WorldPart, Replicated, net::Seed(seed)));
        doors::spawn_doors(&mut commands, &map);
        commands.insert_resource(paths::Walkable::new(&map));
        wraith::spawn_wraith(&mut commands, &map, seed);
        let spawn = dev::tile("BB_SPAWN").unwrap_or(map.spawn);
        let starts = crew::starts(&map, spawn, crew.iter().count());
        for ((mut mate, look), at) in crew.iter_mut().zip(starts) {
            let robber = spawn_robber(&mut commands, at, look.clone());
            commands.entity(robber).insert(net::Owner(mate.id));
            mate.ready = false;
            mate.in_round = true;
        }
        for (i, at) in dev::crew(&map, spawn).into_iter().enumerate() {
            let look = crew::Look {
                name: format!("Bot {}", i + 1),
                colour: i + 1,
                outfit: crew::Outfit::ALL[i % 4],
            };
            let bot = spawn_robber(&mut commands, at, look);
            commands.entity(bot).insert(dev::Bot(i as f32 * 1.7));
        }
    }
    let current = Current {
        seed,
        rooms: map.rooms.len(),
        props: map.props.len(),
        lamps: map.lights.len(),
    };
    info!(
        "bank {seed}: {}x{} tiles, {} rooms, {} props, {} lamps",
        map.w, map.h, current.rooms, current.props, current.lamps
    );
    commands.insert_resource(current);
    commands.insert_resource(smash::Bank(map));
}

fn draw_map(
    mut commands: Commands,
    bank: Res<smash::Bank>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<WorldMaterial>>,
    mut world_materials: ResMut<WorldMaterials>,
) {
    render::spawn_map(
        &mut commands,
        &bank.0,
        &mut meshes,
        &mut images,
        &mut materials,
        &mut world_materials,
    );
}

type Me<'a> = (
    &'a Player,
    &'a Controls,
    &'a guns::Gunplay,
    &'a inventory::Inventory,
    &'a sanity::Sanity,
    Has<player::Dead>,
);

#[allow(clippy::too_many_arguments)]
fn hud(
    current: Option<Res<Current>>,
    level: Option<Res<Level>>,
    me: Single<Me, With<LocalPlayer>>,
    wraiths: Query<&wraith::Wraith>,
    diagnostics: Res<DiagnosticsStore>,
    mut text: Query<&mut Text, With<Hud>>,
) {
    let (Some(current), Some(level), Ok(mut text)) = (current, level, text.single_mut()) else {
        return;
    };
    let (player, controls, gun, inventory, sanity, dead) = *me;
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let b = &player.body;
    let state = if b.is_rolling() {
        "rolling"
    } else if level.hideout(b.pos).is_some() {
        "hidden in the plants"
    } else if level.in_cover(b) {
        "crouching, in cover"
    } else if b.is_crouching() {
        "crouching"
    } else if b.is_sprinting() {
        "sprinting"
    } else if b.speed > 1.0 {
        "walking"
    } else {
        "standing"
    };
    let torch = if controls.torch { "on" } else { "off" };
    let mind = if dead {
        "DEAD: you walk on unseen".to_string()
    } else {
        format!("sanity {:.0}%", sanity.0)
    };
    // What the crew has of the wraith: the letters found, then its name.
    let hunt = wraiths.iter().next().map_or(String::new(), |w| {
        let mut letters: Vec<String> = w.letters.iter().map(char::to_string).collect();
        letters.resize(wraith::NAME_LEN, "_".into());
        match (w.caught, w.named, w.letters.len() >= wraith::NAME_LEN) {
            (true, ..) => "CAUGHT IT: the round is won (F5 for a new bank)".into(),
            (_, true, _) => "its name is spoken: lure it over a trap".into(),
            (_, _, true) => format!("letters {}: T to speak its name", letters.join(" ")),
            _ => format!("letters {}", letters.join(" ")),
        }
    });
    let roll = if b.roll_cooling() > 0.0 {
        "cooling"
    } else {
        "ready"
    };
    let pistol = match inventory.held().map(|i| &inventory.items[i]) {
        Some(p) if p.kind == inventory::Kind::Pistol => {
            let spare: u32 = inventory
                .items
                .iter()
                .filter(|i| i.kind == inventory::Kind::Rounds)
                .map(|i| i.count)
                .sum();
            let state = if gun.reload > 0.0 { "  reloading" } else { "" };
            format!("   pistol {} | {spare}{state}", p.count)
        }
        _ => String::new(),
    };
    text.0 = format!(
        "BANK BOBBERY   bank #{}  ({} rooms, {} props, {} lamps)   {:.0} fps\n\
         WASD walk | mouse aim | Shift sprint | C crouch | Space/Alt roll | F flashlight | Tab inventory | 1-4 quick slots\n\
         E pick up | T speak | R reload | wheel zoom | Esc menu | F11 full screen\n\
         {mind}   {state}   roll {roll}   flashlight {torch}{pistol}\n\
         {hunt}",
        current.seed, current.rooms, current.props, current.lamps, fps
    );
}
