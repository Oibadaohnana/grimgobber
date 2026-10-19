//! The end of a round won: the server counts, every `EVERY` seconds, the
//! tile the wraith is on and each living robber's (`Tracks`), and when the
//! wraith is caught sends the time and both counts (`Ended`). Each player
//! then sees the end screen: the time, and a heatmap over the bank's plan
//! of where it went and where they went. It stays, into the lobby, till
//! closed (Enter, Esc or a click).

use crate::map::{Wall, tile_of};
use crate::player::{Dead, Player};
use crate::smash::Bank;
use crate::wraith::{Mind, Wraith};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_replicon::prelude::ClientState;
use serde::{Deserialize, Serialize};

const EVERY: f32 = 0.25;
/// Screen pixels a tile in the heatmap.
const PIXELS: f32 = 7.0;

/// The round won: its length, the wraith's name and kind, and the tiles it
/// and the robbers were counted on.
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct Ended {
    secs: f32,
    name: String,
    kind: String,
    ghost: Vec<u16>,
    robbers: Vec<u16>,
}

/// The server's counts this round, and when it began.
#[derive(Resource)]
struct Tracks {
    began: f32,
    next: f32,
    ghost: Vec<u16>,
    robbers: Vec<u16>,
    sent: bool,
}

#[derive(Component)]
struct EndRoot;

pub struct EndPlugin;

impl Plugin for EndPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Ended>().add_systems(
            Update,
            track.run_if(resource_exists::<Bank>.and_then(in_state(ClientState::Disconnected))),
        );
        if crate::drawn(app) {
            app.add_systems(Update, show).add_systems(
                PreUpdate,
                close
                    .after(bevy::ui::UiSystems::Focus)
                    .before(crate::menu::escape),
            );
        }
    }
}

#[allow(clippy::type_complexity)]
fn track(
    time: Res<Time>,
    bank: Res<Bank>,
    tracks: Option<ResMut<Tracks>>,
    wraiths: Query<(&Wraith, &Mind)>,
    robbers: Query<&Player, Without<Dead>>,
    mut ended: MessageWriter<Ended>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    let map = &bank.0;
    let Some(mut t) = tracks.filter(|_| !bank.is_added()) else {
        let n = (map.w * map.h) as usize;
        commands.insert_resource(Tracks {
            began: now,
            next: now,
            ghost: vec![0; n],
            robbers: vec![0; n],
            sent: false,
        });
        return;
    };
    let index = |p: Vec2| {
        let (x, y) = tile_of(p);
        map.inside(x, y).then(|| map.idx(x, y))
    };
    if now >= t.next && !t.sent {
        t.next = now + EVERY;
        for (w, _) in &wraiths {
            if let Some(i) = index(w.pos) {
                t.ghost[i] = t.ghost[i].saturating_add(1);
            }
        }
        for p in &robbers {
            if let Some(i) = index(p.body.pos) {
                t.robbers[i] = t.robbers[i].saturating_add(1);
            }
        }
    }
    if let Some((_, m)) = wraiths.iter().find(|(w, _)| w.caught)
        && !t.sent
    {
        t.sent = true;
        ended.write(Ended {
            secs: now - t.began,
            name: m.true_name().into(),
            kind: format!("{:?}", m.ghost),
            ghost: t.ghost.clone(),
            robbers: t.robbers.clone(),
        });
    }
}

/// The heatmap: the bank's plan, the wraith's tiles violet over it, the
/// robbers' orange, each as bright as it was often there (by a log).
fn heatmap(bank: &Bank, e: &Ended) -> Image {
    let map = &bank.0;
    let most = |v: &[u16]| ((*v.iter().max().unwrap_or(&0)).max(1) as f32).ln_1p();
    let (gm, rm) = (most(&e.ghost), most(&e.robbers));
    let mut data = Vec::with_capacity((map.w * map.h * 4) as usize);
    for row in 0..map.h {
        let y = map.h - 1 - row;
        for x in 0..map.w {
            let i = map.idx(x, y);
            let t = map.at(x, y);
            let base = match (t.wall, t.indoor) {
                (Wall::None, true) => Vec3::splat(0.13),
                (Wall::None, false) => Vec3::splat(0.06),
                _ => Vec3::new(0.36, 0.37, 0.42),
            };
            let heat = |v: &[u16], m: f32| (v.get(i).copied().unwrap_or(0) as f32).ln_1p() / m;
            let (g, r) = (heat(&e.ghost, gm), heat(&e.robbers, rm));
            let c = base * (1.0 - 0.85 * g.max(r))
                + Vec3::new(0.72, 0.42, 1.0) * g
                + Vec3::new(1.0, 0.6, 0.2) * r;
            let c = c.min(Vec3::ONE) * 255.0;
            data.extend([c.x as u8, c.y as u8, c.z as u8, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: map.w as u32,
            height: map.h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

fn show(
    mut ended: MessageReader<Ended>,
    bank: Option<Res<Bank>>,
    old: Query<Entity, With<EndRoot>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    let (Some(e), Some(bank)) = (ended.read().last(), bank) else {
        return;
    };
    for o in &old {
        commands.entity(o).despawn();
    }
    let px = Val::Px;
    let text = |s: String, size: f32, c: Color| {
        (
            Text::new(s),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(c),
        )
    };
    let dim = Color::srgb(0.6, 0.62, 0.68);
    let secs = e.secs as u32;
    let key = |c: Color, s: &str| {
        (
            Node {
                column_gap: px(6.0),
                align_items: AlignItems::Center,
                ..default()
            },
            children![
                (
                    Node {
                        width: px(12.0),
                        height: px(12.0),
                        ..default()
                    },
                    BackgroundColor(c)
                ),
                text(s.into(), 13.0, dim)
            ],
        )
    };
    commands.spawn((
        EndRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(10.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.01, 0.01, 0.02, 0.88)),
        GlobalZIndex(40),
        children![
            text("CAUGHT IT".into(), 34.0, Color::srgb(0.85, 0.92, 1.0)),
            text(format!("{}, a {}", e.name, e.kind), 16.0, dim),
            text(
                format!("Time  {:02}:{:02}", secs / 60, secs % 60),
                22.0,
                Color::WHITE
            ),
            (
                Node {
                    width: px(bank.0.w as f32 * PIXELS),
                    height: px(bank.0.h as f32 * PIXELS),
                    border: UiRect::all(px(1.0)),
                    ..default()
                },
                ImageNode::new(images.add(heatmap(&bank, e))),
                BorderColor::all(Color::srgb(0.3, 0.32, 0.36)),
            ),
            (
                Node {
                    column_gap: px(20.0),
                    ..default()
                },
                children![
                    key(Color::srgb(0.72, 0.42, 1.0), "where it went"),
                    key(Color::srgb(1.0, 0.6, 0.2), "where you went")
                ],
            ),
            text("Enter, Esc or a click closes this".into(), 12.0, dim),
        ],
    ));
}

/// Enter, Esc or a click closes it, and nothing else hears that.
fn close(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    script: Res<crate::dev::Script>,
    roots: Query<Entity, With<EndRoot>>,
    mut commands: Commands,
) {
    if roots.is_empty() {
        return;
    }
    let shut = keys.any_just_pressed([KeyCode::Enter, KeyCode::Escape])
        || mouse.just_pressed(MouseButton::Left)
        || script.pressed("Close");
    if shut {
        for e in &roots {
            commands.entity(e).despawn();
        }
        for k in [KeyCode::Enter, KeyCode::Escape] {
            keys.clear_just_pressed(k);
        }
        mouse.clear_just_pressed(MouseButton::Left);
    }
}
