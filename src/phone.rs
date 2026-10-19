//! The phone (P): the six signs to tick on and off as the crew sees them (a
//! click, or 1-6), and every kind of ghost in one box beside how it is
//! caught in another, both struck through once a sign ticked rules it out. While it is open the digits and
//! clicks are its alone. It is dragged about by anything but its rows, and
//! a click on its status bar folds it down to that bar and back.

use crate::dev::Script;
use crate::net::Seed;
use crate::player::Living;
use crate::wraith::{Ghost, Sign};
use bevy::prelude::*;
use bevy::ui::{RelativeCursorPosition, UiSystems};
use bevy::window::PrimaryWindow;

const DIGITS: [KeyCode; 6] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
];

/// Where it starts: its right edge and top from the window's.
const AT: Vec2 = vec2(24.0, 70.0);
const WIDTH: f32 = 310.0;

#[derive(Resource)]
pub struct Phone {
    open: bool,
    seen: [bool; 6],
    folded: bool,
    at: Vec2,
    /// The left button went down on it, and is still held: the pointer is
    /// the phone's (no lantern).
    pub grab: Option<Grab>,
}

impl Default for Phone {
    fn default() -> Self {
        Phone {
            open: false,
            seen: [false; 6],
            folded: false,
            at: AT,
            grab: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Grab {
    pointer: Vec2,
    at: Vec2,
    /// Off its rows, so it drags; on the status bar, so a click folds it.
    drags: bool,
    bar: bool,
    moved: bool,
}

#[derive(Component)]
struct PhoneRoot;

/// The status bar: drags it, and folds it when clicked.
#[derive(Component)]
struct Bar;

/// Everything under the status bar but the home bar, hidden when folded.
#[derive(Component)]
struct Fold;

#[derive(Component)]
struct Row(usize);

pub struct PhonePlugin;

impl Plugin for PhonePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Phone>()
            .add_systems(Startup, spawn)
            .add_systems(
                PreUpdate,
                phone
                    .after(UiSystems::Focus)
                    .after(crate::talk::talk)
                    .before(crate::inventory_ui::input)
                    .before(crate::player::read_controls),
            )
            .add_systems(Update, show);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn phone(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    script: Res<Script>,
    me: Option<Single<(), Living>>,
    seeds: Query<(), Added<Seed>>,
    rows: Query<(&Interaction, &Row), Changed<Interaction>>,
    over: Query<(&RelativeCursorPosition, Has<Bar>), Or<(With<PhoneRoot>, With<Bar>)>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut phone: ResMut<Phone>,
) {
    if !seeds.is_empty() {
        phone.seen = [false; 6];
    }
    if me.is_none() {
        phone.open = false;
    }
    if keys.just_pressed(KeyCode::KeyP) || script.pressed("Phone") {
        phone.open = me.is_some() && !phone.open;
    }
    // Read before written: a `ResMut` touched every frame would redraw it every frame.
    if (!phone.open || !mouse.pressed(MouseButton::Left))
        && let Some(g) = phone.grab
    {
        phone.grab = None;
        phone.folded ^= g.bar && !g.moved;
    }
    if !phone.open {
        return;
    }
    let pointer = window.cursor_position();
    let on = |bar: bool| over.iter().any(|(r, b)| b == bar && r.cursor_over);
    if mouse.just_pressed(MouseButton::Left)
        && let Some(pointer) = pointer
        && on(false)
    {
        let on_row = rows.iter().any(|(i, _)| *i == Interaction::Pressed);
        phone.grab = Some(Grab {
            pointer,
            at: phone.at,
            drags: !on_row,
            bar: on(true),
            moved: false,
        });
    }
    if let (Some(g), Some(pointer)) = (phone.grab, pointer)
        && g.drags
    {
        let d = pointer - g.pointer;
        if d.length() > 4.0 || g.moved {
            // Its right edge and top: kept where some of it can be got hold of.
            let size = window.size();
            let at = vec2(g.at.x - d.x, g.at.y + d.y)
                .clamp(vec2(60.0 - WIDTH, 0.0), size - vec2(60.0, 40.0));
            phone.at = at;
            phone.grab = Some(Grab { moved: true, ..g });
        }
    }
    for (i, key) in DIGITS.into_iter().enumerate() {
        if keys.just_pressed(key) || script.pressed(&format!("Sign{}", i + 1)) {
            phone.seen[i] ^= true;
        }
        keys.reset(key);
    }
    for (interaction, row) in &rows {
        if *interaction == Interaction::Pressed {
            phone.seen[row.0] ^= true;
        }
    }
    mouse.clear_just_pressed(MouseButton::Left);
}

const BODY: Color = Color::srgb(0.07, 0.07, 0.08);
const SCREEN: Color = Color::srgb(0.035, 0.05, 0.085);
const INK: Color = Color::srgb(0.88, 0.9, 0.95);
const DIM: Color = Color::srgb(0.5, 0.54, 0.62);
const ACCENT: Color = Color::srgb(0.55, 0.42, 1.0);

/// A sign's tick box, by its place in `Sign::ALL`.
#[derive(Component)]
struct Check(usize);

/// A kind of ghost's two boxes (its name, how it is caught), and their
/// texts, by its place in `Ghost::ALL`.
#[derive(Component)]
struct GhostBox(usize);

#[derive(Component)]
struct GhostText(usize);

/// The line drawn through a box once its ghost is ruled out.
#[derive(Component)]
struct Strike(usize);

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn text(s: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (Text::new(s), font(size), TextColor(colour))
}

fn rounded(w: Val, h: Val, r: f32) -> Node {
    Node {
        width: w,
        height: h,
        border_radius: BorderRadius::all(Val::Px(r)),
        ..default()
    }
}

/// A handset: a bezel round a screen with a status bar, the signs to tick,
/// and each kind of ghost beside how it is caught.
fn spawn(mut commands: Commands) {
    let px = Val::Px;
    let row = |gap: f32| Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(gap),
        ..default()
    };
    commands
        .spawn((
            PhoneRoot,
            RelativeCursorPosition::default(),
            Node {
                position_type: PositionType::Absolute,
                right: px(AT.x),
                top: px(AT.y),
                width: px(WIDTH),
                padding: UiRect::all(px(9.0)),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(42.0)),
                ..default()
            },
            BackgroundColor(BODY),
            BorderColor::all(Color::srgb(0.32, 0.33, 0.36)),
            GlobalZIndex(10),
            Visibility::Hidden,
        ))
        .with_children(|body| {
            body.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5.0),
                    padding: UiRect::new(px(14.0), px(14.0), px(10.0), px(10.0)),
                    border_radius: BorderRadius::all(px(34.0)),
                    ..default()
                },
                BackgroundColor(SCREEN),
            ))
            .with_children(|s| {
                // The status bar, the camera's island in its middle.
                s.spawn((
                    Bar,
                    RelativeCursorPosition::default(),
                    Node {
                        justify_content: JustifyContent::SpaceBetween,
                        padding: UiRect::vertical(px(4.0)),
                        ..row(0.0)
                    },
                ))
                .with_children(|bar| {
                    bar.spawn(text("GRIM", 11.0, INK));
                    bar.spawn((
                        rounded(px(70.0), px(18.0), 9.0),
                        BackgroundColor(Color::BLACK),
                    ));
                    bar.spawn(row(3.0)).with_children(|r| {
                        for h in [4.0, 6.0, 8.0, 10.0] {
                            r.spawn((
                                Node {
                                    align_self: AlignSelf::FlexEnd,
                                    ..rounded(px(3.0), px(h), 1.0)
                                },
                                BackgroundColor(INK),
                            ));
                        }
                        r.spawn((
                            Node {
                                margin: UiRect::left(px(4.0)),
                                padding: UiRect::all(px(1.5)),
                                border: UiRect::all(px(1.0)),
                                ..rounded(px(20.0), px(10.0), 3.0)
                            },
                            BorderColor::all(INK),
                        ))
                        .with_child((
                            rounded(Val::Percent(70.0), Val::Percent(100.0), 1.5),
                            BackgroundColor(INK),
                        ));
                    });
                });
                s.spawn((
                    Fold,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(5.0),
                        margin: UiRect::top(px(2.0)),
                        ..default()
                    },
                ))
                .with_children(|s| {
                    s.spawn(text("Ghost ID", 20.0, INK));
                    s.spawn(text("Tick what you have seen (click or 1-6)", 11.0, DIM));
                    s.spawn((
                        text("EVIDENCE", 10.0, DIM),
                        Node {
                            margin: UiRect::top(px(6.0)),
                            ..default()
                        },
                    ));
                    for (i, sign) in Sign::ALL.into_iter().enumerate() {
                        s.spawn((
                            Row(i),
                            Button,
                            Node {
                                padding: UiRect::axes(px(10.0), px(6.0)),
                                border_radius: BorderRadius::all(px(10.0)),
                                ..row(10.0)
                            },
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.05)),
                        ))
                        .with_children(|r| {
                            r.spawn((
                                Check(i),
                                Node {
                                    border: UiRect::all(px(1.5)),
                                    ..rounded(px(14.0), px(14.0), 4.0)
                                },
                                BorderColor::all(DIM),
                            ));
                            r.spawn((
                                text(sign.name(), 13.0, INK),
                                Node {
                                    flex_grow: 1.0,
                                    ..default()
                                },
                            ));
                            r.spawn(text(format!("{}", i + 1), 11.0, DIM));
                        });
                    }
                    s.spawn((Node {
                        margin: UiRect::top(px(8.0)),
                        ..row(6.0)
                    },))
                        .with_children(|h| {
                            h.spawn((
                                text("GHOST", 10.0, DIM),
                                Node {
                                    width: px(104.0),
                                    ..default()
                                },
                            ));
                            h.spawn(text("CAPTURE", 10.0, DIM));
                        });
                    for (i, g) in Ghost::ALL.into_iter().enumerate() {
                        s.spawn(Node {
                            align_items: AlignItems::Stretch,
                            ..row(6.0)
                        })
                        .with_children(|r| {
                            let card = |w: Val| {
                                (
                                    GhostBox(i),
                                    Node {
                                        width: w,
                                        // The name's box keeps its width; the other
                                        // takes the rest, its text wrapping in it.
                                        flex_grow: if w == Val::Auto { 1.0 } else { 0.0 },
                                        flex_shrink: if w == Val::Auto { 1.0 } else { 0.0 },
                                        flex_basis: if w == Val::Auto {
                                            px(0.0)
                                        } else {
                                            Val::Auto
                                        },
                                        flex_direction: FlexDirection::Column,
                                        justify_content: JustifyContent::Center,
                                        padding: UiRect::axes(px(8.0), px(6.0)),
                                        border: UiRect::all(px(1.0)),
                                        border_radius: BorderRadius::all(px(10.0)),
                                        ..default()
                                    },
                                    BackgroundColor(Color::NONE),
                                    BorderColor::all(Color::NONE),
                                )
                            };
                            let strike = || {
                                (
                                    Strike(i),
                                    Node {
                                        position_type: PositionType::Absolute,
                                        left: px(6.0),
                                        right: px(6.0),
                                        top: Val::Percent(50.0),
                                        height: px(2.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.75, 0.3, 0.35, 0.8)),
                                    Visibility::Hidden,
                                )
                            };
                            r.spawn(card(px(104.0))).with_children(|c| {
                                c.spawn((GhostText(i), text(format!("{g:?}"), 13.0, INK)));
                                c.spawn(strike());
                            });
                            r.spawn(card(Val::Auto)).with_children(|c| {
                                c.spawn((GhostText(i), text(g.tool(), 13.0, INK)));
                                c.spawn((GhostText(i), text(g.catch(), 11.0, DIM)));
                                c.spawn(strike());
                            });
                        });
                    }
                });
                // The home bar.
                s.spawn((
                    Node {
                        align_self: AlignSelf::Center,
                        margin: UiRect::top(px(10.0)),
                        ..rounded(px(100.0), px(4.0), 2.0)
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
                ));
            });
        });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show(
    phone: Res<Phone>,
    root: Single<(&mut Visibility, &mut Node), With<PhoneRoot>>,
    mut fold: Single<&mut Node, (With<Fold>, Without<PhoneRoot>)>,
    mut rows: Query<(&Row, &mut BackgroundColor), Without<Check>>,
    mut checks: Query<(&Check, &mut BackgroundColor, &mut BorderColor), Without<Row>>,
    mut boxes: Query<
        (&GhostBox, &mut BackgroundColor, &mut BorderColor),
        (Without<Row>, Without<Check>),
    >,
    mut texts: Query<(&GhostText, &mut TextColor, &TextFont)>,
    mut strikes: Query<(&Strike, &mut Visibility), Without<PhoneRoot>>,
) {
    let (mut vis, mut node) = root.into_inner();
    vis.set_if_neq(if phone.open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !phone.is_changed() {
        return;
    }
    node.right = Val::Px(phone.at.x);
    node.top = Val::Px(phone.at.y);
    fold.display = if phone.folded {
        Display::None
    } else {
        Display::Flex
    };
    for (row, mut bg) in &mut rows {
        let a = if phone.seen[row.0] { 0.14 } else { 0.05 };
        bg.0 = Color::srgba(1.0, 1.0, 1.0, a);
    }
    for (check, mut bg, mut border) in &mut checks {
        let seen = phone.seen[check.0];
        bg.0 = if seen { ACCENT } else { Color::NONE };
        *border = BorderColor::all(if seen { ACCENT } else { DIM });
    }
    // Crossed out unless it shows every sign ticked.
    let fits = |g: usize| {
        let signs = Ghost::ALL[g].signs();
        (0..6).all(|i| !phone.seen[i] || signs.contains(&Sign::ALL[i]))
    };
    for (b, mut bg, mut border) in &mut boxes {
        let on = fits(b.0);
        bg.0 = Color::srgba(1.0, 1.0, 1.0, if on { 0.07 } else { 0.02 });
        *border = BorderColor::all(Color::srgba(1.0, 1.0, 1.0, if on { 0.18 } else { 0.05 }));
    }
    for (t, mut colour, font) in &mut texts {
        let on = fits(t.0);
        let big = matches!(font.font_size, FontSize::Px(s) if s > 12.0);
        colour.0 = match (on, big) {
            (true, true) => INK,
            (true, false) => DIM,
            (false, _) => Color::srgba(0.5, 0.52, 0.58, 0.45),
        };
    }
    for (strike, mut vis) in &mut strikes {
        vis.set_if_neq(if fits(strike.0) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}
