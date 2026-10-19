//! Sanity, per robber, 0 to 100: it drains in the bank's dark rooms (slowly
//! in lit ones, not at all outside) and when the wraith shows itself, and the
//! lower it is, the likelier a roll of the wraith's comes up a haunt. The
//! server keeps it; his own shows in a bar beside the quick slots.

use crate::map::tile_of;
use crate::player::{Dead, LocalPlayer, Player};
use crate::sight::Sight;
use crate::smash::Bank;
use bevy::prelude::*;
use bevy_replicon::prelude::ClientState;
use serde::{Deserialize, Serialize};

/// Lost a second, in percent, in a dark room and in a lit one.
const DARK_DRAIN: f32 = 0.25;
const LIT_DRAIN: f32 = 0.05;

#[derive(Component, Clone, Copy, Serialize, Deserialize)]
pub struct Sanity(pub f32);

impl Default for Sanity {
    fn default() -> Self {
        Sanity(100.0)
    }
}

impl Sanity {
    pub fn lose(&mut self, percent: f32) {
        self.0 = (self.0 - percent).max(0.0);
    }
}

pub struct SanityPlugin;

impl Plugin for SanityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (start, drain).chain().distributive_run_if(
                resource_exists::<Bank>
                    .and_then(resource_exists::<Sight>)
                    .and_then(in_state(ClientState::Disconnected)),
            ),
        );
        if crate::drawn(app) {
            app.add_systems(Startup, spawn_bar).add_systems(Update, bar);
        }
    }
}

#[derive(Component)]
struct SanityBar;

#[derive(Component)]
struct Fill;

#[derive(Component)]
struct Percent;

/// Beside the quick slots (which reach 132 px right of the middle), as
/// tall as they are, in their colours.
fn spawn_bar(mut commands: Commands) {
    let px = Val::Px;
    let font = |size| TextFont {
        font_size: FontSize::Px(size),
        ..default()
    };
    let dim = Color::srgb(0.6, 0.63, 0.68);
    commands.spawn((
        SanityBar,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            margin: UiRect::left(px(144.0)),
            bottom: px(18.0),
            width: px(200.0),
            height: px(60.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            row_gap: px(7.0),
            padding: UiRect::axes(px(12.0), px(8.0)),
            border: UiRect::all(px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgb_u8(0x1d, 0x20, 0x24)),
        BorderColor::all(Color::srgb_u8(0x55, 0x5c, 0x66)),
        Visibility::Hidden,
        children![
            (
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                children![
                    (Text::new("SANITY"), font(11.0), TextColor(dim)),
                    (Percent, Text::new(""), font(13.0), TextColor(Color::WHITE)),
                ],
            ),
            (
                Node {
                    height: px(12.0),
                    padding: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::all(px(6.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb_u8(0x13, 0x15, 0x18)),
                children![(
                    Fill,
                    Node {
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                )],
            ),
        ],
    ));
}

/// His sanity: green when sane, amber, then red, pulsing below a quarter.
#[allow(clippy::type_complexity)]
fn bar(
    time: Res<Time>,
    me: Option<Single<(&Sanity, Has<Dead>), With<LocalPlayer>>>,
    mut root: Single<&mut Visibility, With<SanityBar>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<Fill>>,
    mut words: Single<&mut Text, With<Percent>>,
) {
    let Some((sanity, dead)) = me.map(|m| *m) else {
        root.set_if_neq(Visibility::Hidden);
        return;
    };
    root.set_if_neq(Visibility::Inherited);
    let s = if dead { 0.0 } else { sanity.0 };
    let line = if dead {
        "DEAD".into()
    } else {
        format!("{s:.0}%")
    };
    if words.0 != line {
        words.0 = line;
    }
    let (node, colour) = &mut *fill;
    node.width = Val::Percent(s);
    let sane = Color::srgb(0.35, 0.82, 0.55);
    let uneasy = Color::srgb(0.9, 0.7, 0.25);
    let mad = Color::srgb(0.9, 0.28, 0.3);
    let c = if s > 50.0 {
        uneasy.mix(&sane, (s - 50.0) / 50.0)
    } else {
        mad.mix(&uneasy, s / 50.0)
    };
    let pulse = if s < 25.0 {
        0.75 + 0.25 * (time.elapsed_secs() * 6.0).sin()
    } else {
        1.0
    };
    colour.0 = c.with_alpha(pulse);
}

/// `BB_SANITY`: where everyone starts, for trying haunts.
fn start(mut fresh: Query<&mut Sanity, Added<Sanity>>) {
    if let Some(s) = crate::dev::env("BB_SANITY").and_then(|s| s.parse().ok()) {
        for mut sanity in &mut fresh {
            sanity.0 = s;
        }
    }
}

fn drain(
    time: Res<Time>,
    bank: Res<Bank>,
    sight: Res<Sight>,
    mut robbers: Query<(&Player, &mut Sanity), Without<Dead>>,
) {
    let map = &bank.0;
    for (p, mut sanity) in &mut robbers {
        let (x, y) = tile_of(p.body.pos);
        if !map.inside(x, y) || !map.at(x, y).indoor || sanity.0 == 0.0 {
            continue;
        }
        let rate = if sight.lit_at(p.body.pos) {
            LIT_DRAIN
        } else {
            DARK_DRAIN
        };
        sanity.lose(rate * time.delta_secs());
    }
}
