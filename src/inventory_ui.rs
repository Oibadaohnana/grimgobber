//! The inventory screen (Tab): the hands and the inventory's grid, items
//! dragged between them (R turns the one held), and the quick slots along the
//! bottom, always shown. Painted as one mesh fixed to the camera, in logical
//! pixels from the window's middle, `y` up; the words are UI text.

use crate::dev::Script;
use crate::inventory::{At, GRID, Inventory, Item, Kind, size_of};
use crate::paint::{self, Frame, Hex, Painter, hex};
use crate::player::LocalPlayer;
use crate::render::MainCamera;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

const CELL: f32 = 46.0;
const HEAD: f32 = 24.0;
const SLOT: f32 = 60.0;

#[derive(Component)]
struct Screen;

#[derive(Component)]
struct Words;

/// The item being dragged, which way it lies, and where on it the pointer took it.
#[derive(Resource, Default)]
struct Drag {
    id: Option<usize>,
    turned: bool,
    grab: Vec2,
}

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Drag>()
            .add_systems(PostStartup, spawn)
            .add_systems(PreUpdate, input.after(crate::talk::talk))
            .add_systems(Update, draw);
    }
}

struct Layout {
    hands: Rect,
    /// The grid's top-left corner.
    grid: Vec2,
    quick: [Rect; 4],
}

fn layout(win: Vec2) -> Layout {
    let top = 250.0 - HEAD;
    Layout {
        hands: Rect::new(-340.0, top - 2.0 * CELL, -340.0 + 4.0 * CELL, top),
        grid: vec2(40.0, top),
        quick: std::array::from_fn(|i| {
            let x = (i as f32 - 2.0) * (SLOT + 8.0) + 4.0;
            let y = -win.y * 0.5 + 18.0;
            Rect::new(x, y, x + SLOT, y + SLOT)
        }),
    }
}

/// The screen rect of `size` cells from `cell`.
fn cells(lay: &Layout, cell: IVec2, size: IVec2) -> Rect {
    let tl = lay.grid + vec2(cell.x as f32, -cell.y as f32) * CELL;
    let size = size.as_vec2() * CELL;
    Rect::new(tl.x, tl.y - size.y, tl.x + size.x, tl.y)
}

/// Where an item lies on the screen; one in the hands sits in their middle.
fn rect_of(lay: &Layout, item: &Item) -> Rect {
    let size = item.size().as_vec2() * CELL;
    match item.at {
        At::Hands => Rect::from_center_size(lay.hands.center(), size),
        At::Grid(cell) => cells(lay, cell, item.size()),
    }
}

enum Drop {
    At(At),
    Quick(usize),
}

/// What letting go of the dragged item at `p` would do: its top-left, at
/// `p - grab`, snapped to the cell under it.
fn drop_at(lay: &Layout, drag: &Drag, p: Vec2) -> Option<Drop> {
    if lay.hands.contains(p) {
        return Some(Drop::At(At::Hands));
    }
    if let Some(i) = lay.quick.iter().position(|r| r.contains(p)) {
        return Some(Drop::Quick(i));
    }
    if !cells(lay, IVec2::ZERO, GRID).contains(p) {
        return None;
    }
    let tl = p - drag.grab;
    let cell = ((tl - lay.grid) * vec2(1.0, -1.0) / CELL)
        .round()
        .as_ivec2();
    Some(Drop::At(At::Grid(cell)))
}

/// Where a stack's count goes: its rect's bottom-right corner.
fn count_at(r: Rect) -> Vec2 {
    vec2(r.max.x - 20.0, r.min.y + 16.0)
}

fn pointer(window: &Window) -> Option<Vec2> {
    let c = window.cursor_position()?;
    Some(vec2(
        c.x - window.width() * 0.5,
        window.height() * 0.5 - c.y,
    ))
}

fn spawn(
    mut commands: Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<Entity, With<MainCamera>>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    // Bevy's mesh allocator errors on an empty mesh, and `draw` waits for a robber.
    let mut speck = Painter::default();
    speck.rect(Vec2::ZERO, Vec2::ONE, 0.0, hex(0).alpha(0.0));
    commands.entity(camera).with_child((
        Screen,
        Mesh2d(meshes.add(speck.build())),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, 500.0),
        // Rebuilt every frame: its bounds from the first would be stale.
        bevy::camera::visibility::NoFrustumCulling,
    ));
    commands.spawn((
        Words,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
    ));
}

fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    script: Res<Script>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut inv: Single<&mut Inventory, With<LocalPlayer>>,
    mut drag: ResMut<Drag>,
) {
    if keys.just_pressed(KeyCode::Tab) || script.pressed("Inventory") {
        inv.open = !inv.open;
    }
    if keys.just_pressed(KeyCode::Escape) {
        inv.open = false;
    }
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ];
    for (i, key) in digits.into_iter().enumerate() {
        if keys.just_pressed(key) || script.pressed(&format!("Slot{}", i + 1)) {
            inv.quick(i);
        }
    }
    if !inv.open {
        drag.id = None;
        return;
    }
    let Ok(window) = window.single() else {
        return;
    };
    let Some(p) = pointer(window) else {
        return;
    };
    let lay = layout(vec2(window.width(), window.height()));
    let mine = (0..inv.items.len()).find(|&i| rect_of(&lay, &inv.items[i]).contains(p));
    if mouse.just_pressed(MouseButton::Left)
        && let Some(id) = mine
    {
        let r = rect_of(&lay, &inv.items[id]);
        *drag = Drag {
            id: Some(id),
            turned: inv.items[id].turned,
            grab: p - vec2(r.min.x, r.max.y),
        };
    }
    if let Some(id) = drag.id {
        if keys.just_pressed(KeyCode::KeyR) {
            drag.turned = !drag.turned;
            let size = size_of(inv.items[id].kind, drag.turned);
            drag.grab = size.as_vec2() * CELL * vec2(0.5, -0.5);
        }
        if mouse.just_released(MouseButton::Left) {
            match drop_at(&lay, &drag, p) {
                Some(Drop::At(at)) => {
                    inv.put(id, drag.turned, at);
                }
                Some(Drop::Quick(i)) => inv.quick[i] = Some(id),
                None => {}
            }
            drag.id = None;
        }
    }
    if mouse.just_pressed(MouseButton::Right)
        && let Some(i) = lay.quick.iter().position(|r| r.contains(p))
    {
        inv.quick[i] = None;
    }
}

const PANEL: Hex = hex(0x1d2024);
const BAR: Hex = hex(0x30353c);
const HOLE: Hex = hex(0x131518);
const TILE: Hex = hex(0x3b414a);
const LINE: Hex = hex(0x555c66);
const GOLD: Hex = hex(0xd8b04c);

fn boxed(g: &mut Painter, r: Rect, edge: Hex, fill: Hex, alpha: f32) {
    g.rect(r.center(), r.half_size(), 0.0, edge.alpha(alpha));
    g.rect(
        r.center(),
        r.half_size() - Vec2::splat(1.0),
        0.0,
        fill.alpha(alpha),
    );
}

/// An item's picture fitted into `r`, standing if `turned`.
fn picture(g: &mut Painter, kind: Kind, turned: bool, r: Rect) {
    let size = kind.size().as_vec2() * CELL;
    let lies = if turned { vec2(size.y, size.x) } else { size };
    let k = (r.size() / lies).min_element().min(1.0);
    let along = if turned { Vec2::NEG_Y } else { Vec2::X };
    g.frame = Frame {
        origin: r.center(),
        x: along * k,
        y: along.perp() * k,
    };
    paint::item(g, kind, size * 0.5);
    g.frame = Frame::IDENTITY;
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    inv: Single<&Inventory, With<LocalPlayer>>,
    drag: Res<Drag>,
    window: Query<&Window, With<PrimaryWindow>>,
    projection: Query<&Projection, With<MainCamera>>,
    mut screen: Query<(&Mesh2d, &mut Transform), With<Screen>>,
    words: Query<Entity, With<Words>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Ok(window), Ok((mesh, mut t)), Ok(words)) =
        (window.single(), screen.single_mut(), words.single())
    else {
        return;
    };
    if let Ok(Projection::Orthographic(o)) = projection.single() {
        t.scale = Vec3::splat(o.scale);
    }
    let inv = *inv;
    let win = vec2(window.width(), window.height());
    let lay = layout(win);
    let p = pointer(window);
    let dragged = drag.id.and_then(|id| inv.items.get(id)).map(|i| i.kind);
    let mut g = Painter::default();
    let mut text: Vec<(Vec2, String, f32, Hex)> = Vec::new();

    if inv.open {
        g.rect(Vec2::ZERO, win * 0.5, 0.0, hex(0x000000).alpha(0.62));
        let mut panel = |g: &mut Painter, title: String, r: Rect| {
            let head = Rect::new(r.min.x - 6.0, r.max.y, r.max.x + 6.0, r.max.y + HEAD);
            boxed(
                g,
                Rect::new(r.min.x - 6.0, r.min.y - 6.0, r.max.x + 6.0, r.max.y),
                PANEL,
                PANEL,
                0.92,
            );
            boxed(g, head, LINE, BAR, 0.95);
            text.push((
                vec2(head.min.x + 8.0, head.max.y - 4.0),
                title,
                13.0,
                hex(0xd8dcd2),
            ));
        };
        panel(&mut g, "HANDS".into(), lay.hands);
        boxed(&mut g, lay.hands, LINE, HOLE, 0.9);
        panel(
            &mut g,
            format!("INVENTORY  {}x{}", GRID.x, GRID.y),
            cells(&lay, IVec2::ZERO, GRID),
        );
        for y in 0..GRID.y {
            for x in 0..GRID.x {
                let r = cells(&lay, ivec2(x, y), IVec2::ONE);
                boxed(&mut g, r, hex(0x24282d), HOLE, 0.95);
            }
        }
        // Where the dragged item would land, green or red.
        if let (Some(id), Some(p), Some(kind)) = (drag.id, p, dragged)
            && let Some(Drop::At(At::Grid(cell))) = drop_at(&lay, &drag, p)
        {
            let ok = inv.fits(id, drag.turned, cell);
            let r = cells(&lay, cell, size_of(kind, drag.turned));
            let tint = if ok { hex(0x4caf50) } else { hex(0xc0392b) };
            g.rect(r.center(), r.half_size(), 0.0, tint.alpha(0.35));
        }
        let hover = p
            .and_then(|p| (0..inv.items.len()).find(|&i| rect_of(&lay, &inv.items[i]).contains(p)));
        for (i, item) in inv.items.iter().enumerate() {
            if Some(i) == drag.id {
                continue;
            }
            let r = rect_of(&lay, item);
            let edge = if Some(i) == hover { GOLD } else { LINE };
            boxed(
                &mut g,
                Rect::from_center_half_size(r.center(), r.half_size() - Vec2::splat(2.0)),
                edge,
                TILE,
                0.9,
            );
            picture(&mut g, item.kind, item.turned, r);
            if item.kind.counted() && (item.count > 1 || item.kind == Kind::Pistol) {
                text.push((count_at(r), format!("x{}", item.count), 12.0, hex(0xf0ead8)));
            }
        }
        if let (Some(i), None) = (hover, drag.id)
            && let Some(p) = p
        {
            let slot = inv.quick.iter().position(|&q| q == Some(i));
            let bound = slot.map_or(String::new(), |s| format!("  [{}]", s + 1));
            text.push((
                p + vec2(14.0, -14.0),
                format!("{}{bound}", inv.items[i].kind.name()),
                14.0,
                hex(0xf0ead8),
            ));
        }
        text.push((
            vec2(-340.0, lay.hands.min.y - 20.0),
            "Drag to move, R turns it\nDrop on 1-4 to bind, right-click clears\nTab or Esc closes"
                .into(),
            12.0,
            hex(0x9aa0a6),
        ));
    }

    // The quick slots, always.
    for (i, r) in lay.quick.iter().enumerate() {
        let item = inv.quick[i].map(|id| inv.items[id]);
        let held = item.is_some_and(|it| it.at == At::Hands);
        boxed(
            &mut g,
            *r,
            if held { GOLD } else { LINE },
            hex(0x1a1c20),
            0.8,
        );
        if let Some(it) = item {
            picture(
                &mut g,
                it.kind,
                false,
                Rect::from_center_half_size(r.center(), r.half_size() - Vec2::splat(6.0)),
            );
            if it.kind.counted() && (it.count > 1 || it.kind == Kind::Pistol) {
                text.push((count_at(*r), format!("x{}", it.count), 12.0, hex(0xf0ead8)));
            }
        }
        text.push((
            vec2(r.min.x + 4.0, r.max.y - 2.0),
            (i + 1).to_string(),
            12.0,
            hex(0xd8dcd2),
        ));
    }

    if let (Some(kind), Some(p)) = (dragged, p) {
        let size = size_of(kind, drag.turned).as_vec2() * CELL;
        let tl = p - drag.grab;
        let r = Rect::new(tl.x, tl.y - size.y, tl.x + size.x, tl.y);
        boxed(&mut g, r, GOLD, TILE, 0.75);
        picture(&mut g, kind, drag.turned, r);
    }

    if let Some(mut m) = meshes.get_mut(&mesh.0) {
        *m = g.build();
    }
    commands
        .entity(words)
        .despawn_related::<Children>()
        .with_children(|w| {
            for (at, s, size, colour) in text {
                let [r, gr, b, a] = colour.paint().rgba;
                w.spawn((
                    Text::new(s),
                    TextFont {
                        font_size: FontSize::Px(size),
                        ..default()
                    },
                    TextColor(Color::linear_rgba(r, gr, b, a)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(win.x * 0.5 + at.x),
                        top: Val::Px(win.y * 0.5 - at.y),
                        ..default()
                    },
                ));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_item_lands_on_the_cell_under_it() {
        let lay = layout(vec2(1400.0, 900.0));
        // The lockpicks, taken by their middle, let go over cell (3, 2).
        let drag = Drag {
            id: Some(2),
            turned: false,
            grab: vec2(0.5, -0.5) * CELL,
        };
        let p = lay.grid + vec2(3.4, -2.6) * CELL;
        let landed = drop_at(&lay, &drag, p);
        assert!(matches!(landed, Some(Drop::At(At::Grid(cell))) if cell == ivec2(3, 2)));
        let hands = drop_at(&lay, &drag, lay.hands.center());
        assert!(matches!(hands, Some(Drop::At(At::Hands))));
        let slot = drop_at(&lay, &drag, lay.quick[3].center());
        assert!(matches!(slot, Some(Drop::Quick(3))));
    }
}
