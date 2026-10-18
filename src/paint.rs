use crate::crew::{Look, Outfit};
use crate::inventory::Kind;
use crate::map::{DoorKind, NORTH, Prop, PropKind, SOUTH, face_dir};
use crate::movement::TILE;
use crate::rng::Rng;
use bevy::asset::RenderAssetUsages;
use bevy::math::{Vec2, vec2};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use std::f32::consts::{PI, TAU};

/// Linear RGBA. A glowing vertex (UV x = 1) ignores light.
#[derive(Clone, Copy, Debug)]
pub struct Paint {
    pub rgba: [f32; 4],
    pub glow: bool,
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub const fn hex(rgb: u32) -> Hex {
    Hex(rgb)
}

/// An sRGB colour.
#[derive(Clone, Copy, Debug)]
pub struct Hex(pub u32);

impl Hex {
    pub fn paint(self) -> Paint {
        self.alpha(1.0)
    }
    pub fn alpha(self, a: f32) -> Paint {
        let ch = |s: u32| to_linear(((self.0 >> s) & 0xff) as f32 / 255.0);
        Paint {
            rgba: [ch(16), ch(8), ch(0), a],
            glow: false,
        }
    }
    pub fn glow(self, power: f32) -> Paint {
        let mut p = self.paint();
        for c in &mut p.rgba[..3] {
            *c *= power;
        }
        p.glow = true;
        p
    }
    /// Towards white (`amt` > 0) or black (`amt` < 0).
    pub fn shade(self, amt: f32) -> Hex {
        let ch = |s: u32| -> u32 {
            let c = ((self.0 >> s) & 0xff) as f32;
            let t = if amt < 0.0 { 0.0 } else { 255.0 };
            (c + (t - c) * amt.abs()).round().clamp(0.0, 255.0) as u32
        };
        Hex((ch(16) << 16) | (ch(8) << 8) | ch(0))
    }
}

impl From<Hex> for Paint {
    fn from(h: Hex) -> Paint {
        h.paint()
    }
}

/// A prop's own axes: `x` across its front, `y` out of its front.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: Vec2,
    pub x: Vec2,
    pub y: Vec2,
}

impl Frame {
    pub const IDENTITY: Frame = Frame {
        origin: Vec2::ZERO,
        x: Vec2::X,
        y: Vec2::Y,
    };
    pub fn at(&self, p: Vec2) -> Vec2 {
        self.origin + self.x * p.x + self.y * p.y
    }
}

impl Default for Frame {
    fn default() -> Self {
        Frame::IDENTITY
    }
}

#[derive(Default)]
pub struct Painter {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    pub frame: Frame,
    /// Out of sight it is gone (UV y = 1), not drawn under the fog: people move.
    pub fades: bool,
}

impl Painter {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    fn vertex(&mut self, p: Vec2, paint: Paint) -> u32 {
        let w = self.frame.at(p);
        self.positions.push([w.x, w.y, 0.0]);
        self.colors.push(paint.rgba);
        self.uvs
            .push([paint.glow, self.fades].map(|b| if b { 1.0 } else { 0.0 }));
        (self.positions.len() - 1) as u32
    }

    /// A convex polygon, as a fan.
    pub fn poly(&mut self, points: &[Vec2], paint: impl Into<Paint>) {
        let paint = paint.into();
        if points.len() < 3 {
            return;
        }
        let first = self.vertex(points[0], paint);
        let mut prev = self.vertex(points[1], paint);
        for &p in &points[2..] {
            let v = self.vertex(p, paint);
            self.indices.extend([first, prev, v]);
            prev = v;
        }
    }

    pub fn rect(&mut self, centre: Vec2, half: Vec2, angle: f32, paint: impl Into<Paint>) {
        let (s, c) = angle.sin_cos();
        let ax = vec2(c, s) * half.x;
        let ay = vec2(-s, c) * half.y;
        let corners = [
            centre - ax - ay,
            centre + ax - ay,
            centre + ax + ay,
            centre - ax + ay,
        ];
        self.poly(&corners, paint);
    }

    pub fn rounded(&mut self, centre: Vec2, half: Vec2, radius: f32, paint: impl Into<Paint>) {
        let r = radius.min(half.x).min(half.y);
        let mut pts = Vec::new();
        for (k, (sx, sy)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let c = centre + vec2(sx * (half.x - r), sy * (half.y - r));
            for i in 0..=4 {
                let a = (k as f32 + i as f32 / 4.0) * PI / 2.0;
                pts.push(c + vec2(a.cos(), a.sin()) * r);
            }
        }
        self.poly(&pts, paint);
    }

    pub fn ellipse(&mut self, centre: Vec2, radii: Vec2, angle: f32, paint: impl Into<Paint>) {
        let n = ((radii.x.max(radii.y) * 0.9) as usize).clamp(10, 40);
        let (s, c) = angle.sin_cos();
        let pts: Vec<Vec2> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * TAU;
                let p = vec2(a.cos() * radii.x, a.sin() * radii.y);
                centre + vec2(p.x * c - p.y * s, p.x * s + p.y * c)
            })
            .collect();
        self.poly(&pts, paint);
    }

    pub fn circle(&mut self, centre: Vec2, r: f32, paint: impl Into<Paint>) {
        self.ellipse(centre, Vec2::splat(r), 0.0, paint);
    }

    pub fn line(&mut self, a: Vec2, b: Vec2, width: f32, paint: impl Into<Paint>) {
        let d = b - a;
        let len = d.length();
        if len < 1e-4 {
            return;
        }
        let n = vec2(-d.y, d.x) / len * (width * 0.5);
        self.poly(&[a - n, b - n, b + n, a + n], paint);
    }

    pub fn capsule(&mut self, a: Vec2, b: Vec2, width: f32, paint: impl Into<Paint>) {
        let paint = paint.into();
        self.line(a, b, width, paint);
        self.circle(a, width * 0.5, paint);
        self.circle(b, width * 0.5, paint);
    }

    pub fn shadow(&mut self, centre: Vec2, half: Vec2, radius: f32) {
        for (grow, a) in [(5.0, 0.10), (2.5, 0.14), (0.0, 0.18)] {
            let half = half + Vec2::splat(grow);
            let paint = hex(0x000000).alpha(a);
            self.rounded(centre + vec2(3.0, -4.0), half, radius + grow, paint);
        }
    }

    pub fn build(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

mod pal {
    use super::{Hex, hex};
    pub const WOOD: Hex = hex(0x8a5a36);
    pub const WOOD_DARK: Hex = hex(0x5e3a22);
    pub const WALNUT: Hex = hex(0x4a2c1c);
    pub const STEEL: Hex = hex(0x8c949c);
    pub const STEEL_DARK: Hex = hex(0x4c535a);
    pub const BLACK: Hex = hex(0x1c1d21);
    pub const PAPER: Hex = hex(0xeeeae0);
    pub const LEAF: Hex = hex(0x3f7a3a);
    pub const LEAF_DARK: Hex = hex(0x2a5428);
    pub const POT: Hex = hex(0xb06a44);
    pub const MARBLE: Hex = hex(0xd8d2c6);
    pub const CASH: Hex = hex(0x6fa060);
    pub const GOLD: Hex = hex(0xe0b03c);
    pub const SCREEN: Hex = hex(0x6fc8ff);
    pub const VELVET: Hex = hex(0x8c1f2a);
    pub const BRASS: Hex = hex(0xc8a050);
    pub const LEATHER: Hex = hex(0x6a3424);
    pub const WHITE: Hex = hex(0xf2f2ee);
    pub const RUG: Hex = hex(0x7a2830);
}

/// Layers: 0 under everything (rugs), 1 the props, 2 over the player.
pub fn paint_props(props: &[Prop]) -> [Painter; 3] {
    use PropKind::*;
    let mut layers = [Painter::default(), Painter::default(), Painter::default()];
    // Shadows first, so nothing's shadow falls on another's top.
    for p in props {
        if matches!(p.kind, Rug | VaultDoor | Stanchion | StreetLamp | Tree) {
            continue;
        }
        let b = p.body;
        let r = if matches!(p.kind, Plant | Bush | Bin | Pillar | Hydrant) {
            b.width().min(b.height()) * 0.5
        } else {
            3.0
        };
        layers[1].shadow(b.centre(), (b.max - b.min) * 0.5, r);
    }
    for p in props {
        let layer = if p.kind == Rug { 0 } else { 1 };
        paint_prop(p, &mut layers[layer]);
        if matches!(p.kind, Tree | StreetLamp | BigPlant | Thicket) {
            paint_high(p, &mut layers[2]);
        }
    }
    layers
}

/// The prop's frame, and its half-size in it.
fn frame_of(p: &Prop) -> (Frame, Vec2) {
    let y = face_dir(p.face);
    let size = p.body.max - p.body.min;
    let local = if p.face == NORTH || p.face == SOUTH {
        size
    } else {
        vec2(size.y, size.x)
    };
    let frame = Frame {
        origin: p.body.centre(),
        x: vec2(y.y, -y.x),
        y,
    };
    (frame, local * 0.5)
}

fn paint_prop(p: &Prop, g: &mut Painter) {
    use PropKind::*;
    let (frame, h) = frame_of(p);
    g.frame = frame;
    let mut rng = Rng::new(p.seed as u64);
    let o = Vec2::ZERO;
    match p.kind {
        Desk | BigDesk => {
            let big = p.kind == BigDesk;
            let top = if big { pal::WALNUT } else { pal::WOOD };
            g.rounded(o, h, 2.0, top.shade(-0.25));
            g.rounded(vec2(0.0, 1.0), h - Vec2::splat(2.0), 2.0, top);
            let mx = if big {
                -h.x * 0.45
            } else {
                rng.f32() * 10.0 - 5.0
            };
            g.rect(vec2(mx, -h.y + 9.0), vec2(16.0, 3.0), 0.0, pal::BLACK);
            g.rect(vec2(mx, -h.y + 11.0), vec2(14.0, 1.4), 0.0, pal::SCREEN);
            g.rect(vec2(mx, -h.y + 3.0), vec2(4.0, 3.0), 0.0, pal::BLACK);
            g.rect(vec2(mx, 5.0), vec2(11.0, 3.5), 0.0, hex(0x2a2c30));
            for _ in 0..rng.range(1, 4) {
                let at =
                    vec2(rng.f32() * 2.0 - 1.0, rng.f32() * 2.0 - 1.0) * (h - Vec2::splat(10.0));
                g.rect(at, vec2(6.0, 8.0), rng.f32() - 0.5, pal::PAPER);
            }
            if big {
                g.rect(vec2(h.x * 0.5, 2.0), vec2(9.0, 2.0), 0.0, pal::BRASS);
                g.circle(vec2(h.x * 0.75, -6.0), 5.0, hex(0x2a6a9a));
            }
        }
        Chair => {
            let seat = hex(0x2c2f36);
            g.circle(o, 11.0, seat.shade(-0.3));
            g.rounded(vec2(0.0, 1.0), vec2(9.0, 8.0), 4.0, seat.shade(0.1));
            g.rounded(vec2(0.0, -9.0), vec2(10.0, 3.0), 2.0, seat);
        }
        Cabinet => {
            g.rect(o, h, 0.0, pal::STEEL_DARK);
            g.rect(o, h - Vec2::splat(2.0), 0.0, pal::STEEL);
            let x = h.x - 3.0;
            for k in 0..3 {
                let y = -h.y + 5.0 + k as f32 * (h.y * 2.0 - 10.0) / 2.0;
                g.line(vec2(-x, y), vec2(x, y), 1.0, pal::STEEL_DARK);
            }
            g.rect(vec2(0.0, h.y - 3.0), vec2(5.0, 1.2), 0.0, pal::BLACK);
        }
        Bookshelf => {
            g.rect(o, h, 0.0, pal::WOOD_DARK);
            g.rect(o, h - Vec2::splat(2.5), 0.0, pal::WALNUT);
            let mut x = -h.x + 3.0;
            while x < h.x - 4.0 {
                let w = 2.0 + rng.f32() * 3.0;
                let c = rng.pick(&[0x8c2a2a, 0x2a4c8c, 0x2a7a4a, 0xc89a3a, 0x5a3a7a, 0xd0c8b0]);
                let hy = h.y - 4.0 - rng.f32() * 3.0;
                g.rect(vec2(x + w * 0.5, 1.0), vec2(w * 0.5, hy), 0.0, hex(c));
                x += w + 0.6;
            }
        }
        Lockers => {
            g.rect(o, h, 0.0, pal::STEEL_DARK);
            g.rect(o, h - Vec2::splat(2.5), 0.0, pal::STEEL);
            let cols = (h.x * 2.0 / 12.0) as i32;
            let door = pal::STEEL.shade(0.15);
            for i in 0..cols {
                let x = -h.x + 6.0 + i as f32 * (h.x * 2.0 - 12.0) / (cols - 1).max(1) as f32;
                g.rect(vec2(x, 0.0), vec2(4.8, h.y - 3.0), 0.0, door);
                g.circle(vec2(x, h.y - 5.0), 1.2, pal::BRASS);
            }
        }
        Plant => {
            g.circle(o, 12.0, pal::POT.shade(-0.3));
            g.circle(o, 10.0, pal::POT);
            for i in 0..7 {
                let a = i as f32 / 7.0 * TAU + rng.f32();
                let d = 8.0 + rng.f32() * 6.0;
                let c = pal::LEAF.shade(rng.f32() * 0.3 - 0.15);
                g.ellipse(vec2(a.cos(), a.sin()) * d * 0.6, vec2(d, 4.5), a, c);
            }
            g.circle(o, 5.0, pal::LEAF_DARK);
        }
        BigPlant => {
            g.circle(o, 15.5, pal::POT.shade(-0.3));
            g.circle(o, 13.5, pal::POT);
            g.circle(o, 11.0, hex(0x3a2a1e));
            g.circle(o, 4.0, pal::WOOD_DARK);
        }
        // The bed of leaves he stands in; a few more lie over him (`paint_high`).
        Thicket => {
            g.rounded(o, h, 3.0, hex(0x5a554e));
            g.rounded(o, h - Vec2::splat(3.0), 2.0, hex(0x2e2418));
            for k in 0..(h.x * h.y / 22.0) as usize {
                let at = (vec2(rng.f32(), rng.f32()) * 2.0 - 1.0) * (h - Vec2::splat(6.0));
                let leaf = if k % 3 == 0 {
                    pal::LEAF
                } else {
                    pal::LEAF_DARK
                };
                g.circle(
                    at,
                    7.0 + rng.f32() * 6.0,
                    leaf.shade(rng.f32() * 0.2 - 0.15),
                );
            }
        }
        Pillar => {
            g.rect(o, h, 0.0, pal::MARBLE.shade(-0.5));
            g.rect(o, h - Vec2::splat(2.0), 0.0, pal::MARBLE.shade(-0.2));
            g.circle(o, h.x + 1.0, pal::MARBLE.shade(-0.35));
            g.circle(o, h.x - 2.0, pal::MARBLE);
            g.circle(vec2(-4.0, 4.0), h.x * 0.5, pal::MARBLE.shade(0.3));
        }
        Bench => {
            g.rounded(o, h, 4.0, pal::WOOD_DARK);
            let mut x = -h.x + 4.0;
            while x < h.x - 2.0 {
                g.rect(vec2(x + 2.0, 0.0), vec2(2.4, h.y - 2.0), 0.0, pal::WOOD);
                x += 6.0;
            }
            g.rect(vec2(-h.x + 3.0, 0.0), vec2(2.0, h.y), 0.0, pal::BLACK);
            g.rect(vec2(h.x - 3.0, 0.0), vec2(2.0, h.y), 0.0, pal::BLACK);
        }
        Counter => {
            g.rect(o, h, 0.0, pal::WALNUT);
            g.rect(vec2(0.0, 3.0), h - vec2(0.0, 6.0), 0.0, pal::MARBLE);
            g.rect(vec2(0.0, h.y - 2.0), vec2(h.x, 2.0), 0.0, pal::BRASS);
            let glass = hex(0xa8d8e8).alpha(0.6);
            g.rect(vec2(0.0, -2.0), vec2(h.x, 1.0), 0.0, glass);
            let n = (h.x * 2.0 / TILE).round() as i32;
            for k in (1..n).step_by(3) {
                let x = -h.x + (k as f32 + 0.5) * TILE;
                g.rect(vec2(x, -8.0), vec2(10.0, 5.0), 0.0, pal::BLACK);
                g.rect(vec2(x, -10.0), vec2(8.0, 1.5), 0.0, pal::SCREEN);
                g.rect(vec2(x + 14.0, -9.0), vec2(6.0, 4.0), 0.0, hex(0x3a3c40));
            }
        }
        Atm => {
            g.rect(o, h, 0.0, hex(0x2a3a5a));
            g.rect(vec2(0.0, 2.0), h - Vec2::splat(3.0), 0.0, hex(0x3a5a8a));
            g.rect(vec2(0.0, h.y - 7.0), vec2(9.0, 3.0), 0.0, pal::SCREEN);
            g.rect(vec2(0.0, h.y - 2.0), vec2(10.0, 1.0), 0.0, hex(0x6aff8a));
        }
        Cooler => {
            g.rect(o, h, 0.0, hex(0xd8dcdc));
            g.circle(o, h.x * 0.8, hex(0x6ab0e0).alpha(0.9));
            g.circle(vec2(-2.0, 2.0), h.x * 0.3, hex(0xbfe8ff));
        }
        Rack => {
            g.rect(o, h, 0.0, hex(0x18191c));
            g.rect(o, h - Vec2::splat(2.5), 0.0, hex(0x26282c));
            for i in 0..(h.x * 2.0 / 7.0) as i32 {
                let x = -h.x + 4.0 + i as f32 * 7.0;
                g.line(vec2(x, -h.y + 3.0), vec2(x, h.y - 3.0), 0.8, hex(0x0e0f10));
                let led = rng.pick(&[0x40ff70, 0x40ff70, 0xffb040, 0x50a0ff]);
                g.rect(vec2(x + 2.0, h.y - 4.0), vec2(1.1, 1.1), 0.0, hex(led));
                if rng.chance(0.5) {
                    g.rect(vec2(x + 2.0, h.y - 8.0), vec2(1.1, 1.1), 0.0, hex(0x40ff70));
                }
            }
        }
        Table => {
            g.rounded(o, h, 6.0, pal::WOOD.shade(-0.3));
            let top = pal::WOOD.shade(0.1);
            g.rounded(vec2(0.0, 1.0), h - Vec2::splat(2.0), 6.0, top);
            g.circle(vec2(-6.0, 4.0), 4.0, pal::WHITE);
            g.circle(vec2(-6.0, 4.0), 2.6, hex(0x5a3a22));
            g.rect(vec2(8.0, -6.0), vec2(7.0, 5.0), 0.3, pal::PAPER);
        }
        Fridge => {
            g.rect(o, h, 0.0, hex(0xb8bcbe));
            g.rect(o, h - Vec2::splat(2.0), 0.0, hex(0xe6e8e8));
            let x = h.x - 2.0;
            g.line(vec2(-x, -2.0), vec2(x, -2.0), 1.0, hex(0x9a9ea0));
            g.rect(
                vec2(h.x - 5.0, h.y - 3.0),
                vec2(3.0, 1.0),
                0.0,
                pal::STEEL_DARK,
            );
        }
        Vending => {
            g.rect(o, h, 0.0, hex(0x8c1a1a));
            g.rect(
                vec2(-3.0, 2.0),
                vec2(h.x - 7.0, h.y - 4.0),
                0.0,
                hex(0xffe8c0),
            );
            for k in 0..4 {
                let c = rng.pick(&[0xe03030, 0x30a0e0, 0xe0c030, 0x30c060]);
                let x = -h.x + 8.0 + k as f32 * 6.0;
                g.rect(vec2(x, 2.0), vec2(2.0, 2.0), 0.0, hex(c));
            }
        }
        SinkCounter => {
            g.rect(o, h, 0.0, hex(0xc8c4bc));
            let n = (h.x * 2.0 / TILE).round().max(1.0) as i32;
            for k in 0..n {
                let x = -h.x + (k as f32 + 0.5) * TILE;
                g.ellipse(vec2(x, 0.0), vec2(10.0, 7.0), 0.0, hex(0xeeeeee));
                g.ellipse(vec2(x, 0.0), vec2(7.0, 4.5), 0.0, hex(0xb8c4cc));
                g.rect(vec2(x, -h.y + 3.0), vec2(1.5, 3.0), 0.0, pal::STEEL);
            }
        }
        Toilet => {
            let half = TILE * 0.5;
            g.frame.origin = p.block.centre();
            g.rect(vec2(-half + 1.5, 0.0), vec2(1.5, half), 0.0, hex(0x9aa4a8));
            g.rect(vec2(half - 1.5, 0.0), vec2(1.5, half), 0.0, hex(0x9aa4a8));
            g.rect(
                vec2(0.0, -half + 4.0),
                vec2(9.0, 4.0),
                0.0,
                pal::WHITE.shade(-0.1),
            );
            g.ellipse(vec2(0.0, -half + 16.0), vec2(8.0, 10.0), 0.0, pal::WHITE);
            g.ellipse(vec2(0.0, -half + 17.0), vec2(5.0, 6.5), 0.0, hex(0xa8c8d8));
        }
        Pallet => {
            g.rect(o, h, 0.0, hex(0x9a7a4a));
            for (i, j) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let at = vec2(i, j) * (h * 0.48);
                g.rect(at, h * 0.46, 0.0, pal::CASH.shade(-0.2));
                g.rect(at + vec2(0.0, 1.0), h * 0.42, 0.0, pal::CASH);
                g.rect(at, vec2(h.x * 0.1, h.y * 0.42), 0.0, hex(0xe8e0c8));
            }
        }
        Gold => {
            for (at, c) in [
                (vec2(-6.0, -5.0), pal::GOLD),
                (vec2(6.0, -5.0), pal::GOLD),
                (vec2(0.0, 5.0), pal::GOLD.shade(0.2)),
            ] {
                g.rect(at, vec2(7.0, 4.0), 0.0, pal::GOLD.shade(-0.35));
                g.rect(at + vec2(0.0, 0.5), vec2(5.5, 2.8), 0.0, c);
            }
        }
        Safe => {
            g.rect(o, h, 0.0, hex(0x2c3034));
            g.rect(o, h - Vec2::splat(2.5), 0.0, hex(0x40464c));
            g.circle(vec2(0.0, 1.0), 7.0, pal::STEEL);
            g.circle(vec2(0.0, 1.0), 4.0, hex(0x2c3034));
            g.line(vec2(-2.0, 1.0), vec2(2.0, 1.0), 1.2, pal::BRASS);
        }
        MonitorDesk => {
            g.rect(o, h, 0.0, hex(0x2a2c30));
            for k in 0..(h.x * 2.0 / 22.0) as i32 {
                let x = -h.x + 11.0 + k as f32 * 22.0;
                g.rect(vec2(x, -h.y + 7.0), vec2(9.5, 4.0), 0.0, pal::BLACK);
                let c = rng.pick(&[0x7ad0ff, 0x9affb0, 0x7ad0ff, 0xd0d0ff]);
                g.rect(vec2(x, -h.y + 9.0), vec2(8.0, 2.0), 0.0, hex(c));
            }
            g.rect(vec2(0.0, h.y - 6.0), vec2(14.0, 3.0), 0.0, hex(0x18191c));
        }
        Sofa => {
            g.rounded(o, h, 6.0, pal::LEATHER.shade(-0.3));
            g.rounded(
                vec2(0.0, 3.0),
                vec2(h.x - 7.0, h.y - 6.0),
                4.0,
                pal::LEATHER,
            );
            g.line(
                vec2(0.0, -h.y + 6.0),
                vec2(0.0, h.y - 3.0),
                0.8,
                pal::LEATHER.shade(-0.4),
            );
        }
        Copier => {
            g.rect(o, h, 0.0, hex(0xb4b8b8));
            g.rect(
                vec2(0.0, -3.0),
                vec2(h.x - 3.0, h.y - 7.0),
                0.0,
                hex(0xdadcdc),
            );
            g.rect(
                vec2(h.x - 6.0, h.y - 4.0),
                vec2(3.0, 2.0),
                0.0,
                hex(0x60ff90),
            );
        }
        Rug => {
            g.frame.origin = p.block.centre();
            let hb = (p.block.max - p.block.min) * 0.5 - Vec2::splat(6.0);
            g.rect(o, hb, 0.0, pal::RUG.shade(-0.2));
            g.rect(o, hb - Vec2::splat(5.0), 0.0, pal::RUG);
            g.rect(o, hb - Vec2::splat(16.0), 0.0, pal::BRASS.shade(-0.3));
            g.rect(o, hb - Vec2::splat(19.0), 0.0, pal::RUG.shade(0.08));
        }
        Stanchion => {
            g.frame.origin = p.block.centre();
            let post = vec2(-TILE * 0.5, 0.0);
            g.line(post, post + vec2(TILE, 0.0), 2.4, pal::VELVET);
            g.circle(post, 4.5, pal::BRASS.shade(-0.3));
            g.circle(post, 3.0, pal::BRASS);
        }
        VaultDoor => {
            // Swung open into the vault, against its wall.
            g.frame = Frame {
                origin: p.block.centre(),
                ..Frame::IDENTITY
            };
            let into = -face_dir(p.face);
            let across = vec2(into.y, -into.x);
            let hinge = across * TILE + into * 8.0;
            let centre = hinge - across * 6.0 + into * 40.0;
            let angle = into.y.atan2(into.x);
            g.ellipse(centre, vec2(36.0, 9.0), angle, hex(0x6a7078));
            g.ellipse(centre, vec2(33.0, 6.5), angle, hex(0x9aa2aa));
            g.rect(hinge, vec2(5.0, 5.0), 0.0, hex(0x3a3e44));
            for k in -3..=3 {
                g.circle(across * (k as f32 * 13.0) - into * 20.0, 2.4, pal::STEEL);
            }
        }
        Tree => {
            g.circle(o, 9.0, hex(0x4a3424));
            g.circle(o, 6.0, hex(0x5e4430));
        }
        Bush => {
            for i in 0..6 {
                let a = i as f32 / 6.0 * TAU + rng.f32();
                let c = pal::LEAF_DARK.shade(rng.f32() * 0.2);
                g.circle(vec2(a.cos(), a.sin()) * 10.0, 12.0, c);
            }
            g.circle(o, 12.0, pal::LEAF);
        }
        Car | Van => paint_car(g, h, &mut rng, p.kind == Van),
        StreetLamp => {
            g.circle(o, 7.0, hex(0x2a2c30));
            g.circle(o, 4.5, hex(0x40444a));
        }
        Hydrant => {
            g.circle(o, 9.0, hex(0x9a1818));
            g.circle(o, 6.0, hex(0xd02a24));
            g.rect(o, vec2(11.0, 2.5), 0.0, hex(0xb02020));
            g.circle(o, 2.5, pal::BRASS);
        }
        Bin => {
            g.circle(o, 11.0, hex(0x2a3a2a));
            g.circle(o, 9.0, hex(0x3a4c3a));
            g.circle(o, 5.0, hex(0x1a1c1a));
        }
    }
}

/// What stands over the player: a tree's crown, a lamp's head.
fn paint_high(p: &Prop, g: &mut Painter) {
    let mut rng = Rng::new(p.seed as u64 ^ 0xabc);
    g.frame = Frame {
        origin: p.block.centre(),
        ..Frame::IDENTITY
    };
    match p.kind {
        PropKind::Tree => {
            let r = 38.0 + rng.f32() * 12.0;
            g.circle(vec2(6.0, -8.0), r, hex(0x000000).alpha(0.22));
            for i in 0..9 {
                let a = i as f32 / 9.0 * TAU + rng.f32() * 0.5;
                let d = r * (0.45 + rng.f32() * 0.2);
                let c = pal::LEAF_DARK.shade(rng.f32() * 0.1);
                g.circle(vec2(a.cos(), a.sin()) * d, r * 0.5, c);
            }
            g.circle(Vec2::ZERO, r * 0.7, pal::LEAF);
            for _ in 0..5 {
                let at = vec2(rng.f32() - 0.5, rng.f32() - 0.5) * r * 0.9;
                g.circle(at, r * 0.25, pal::LEAF.shade(0.12));
            }
        }
        PropKind::BigPlant => {
            g.circle(vec2(5.0, -7.0), 30.0, hex(0x000000).alpha(0.18));
            for i in 0..9 {
                let a = i as f32 / 9.0 * TAU + rng.f32() * 0.4;
                let len = 24.0 + rng.f32() * 9.0;
                let dir = Vec2::from_angle(a);
                let c = pal::LEAF_DARK.shade(rng.f32() * 0.25 - 0.05);
                g.ellipse(dir * len * 0.5, vec2(len * 0.55, 5.5), a, c);
                g.line(dir * 3.0, dir * (len - 3.0), 1.2, pal::LEAF.shade(0.15));
            }
            g.circle(Vec2::ZERO, 6.0, pal::LEAF.shade(-0.2));
        }
        // Tall leaves over whoever hides in it, loose enough to see him.
        PropKind::Thicket => {
            let h = (p.block.max - p.block.min) * 0.5;
            for _ in 0..(h.x * h.y / 120.0) as usize {
                let at = (vec2(rng.f32(), rng.f32()) * 2.0 - 1.0) * (h - Vec2::splat(4.0));
                let a = rng.f32() * TAU;
                let leaf = pal::LEAF.shade(rng.f32() * 0.25 - 0.1).alpha(0.85);
                g.ellipse(at, vec2(9.0 + rng.f32() * 5.0, 3.5), a, leaf);
            }
        }
        PropKind::StreetLamp => {
            let head = vec2(0.0, -12.0);
            g.line(Vec2::ZERO, head, 4.0, hex(0x2a2c30));
            g.rounded(head, vec2(12.0, 6.0), 3.0, hex(0x30343a));
            g.rounded(head, vec2(9.0, 3.5), 2.0, hex(0xffc070).glow(4.0));
        }
        _ => {}
    }
}

fn paint_car(g: &mut Painter, h: Vec2, rng: &mut Rng, van: bool) {
    let colour = if van {
        hex(0xe8e6e0)
    } else {
        hex(rng.pick(&[0x8a1c1c, 0x1c3a6a, 0x2a2a2e, 0xc8c8c4, 0x3a5a3a, 0xb07a2a]))
    };
    let glass = hex(0x2a3440);
    g.rounded(Vec2::ZERO, h, 10.0, colour.shade(-0.4));
    g.rounded(Vec2::ZERO, h - Vec2::splat(2.0), 9.0, colour);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let at = vec2(sx * (h.x - 1.0), sy * h.y * 0.62);
        g.rect(at, vec2(3.0, 11.0), 0.0, pal::BLACK);
    }
    if van {
        g.rounded(vec2(0.0, h.y - 34.0), vec2(h.x - 6.0, 16.0), 6.0, glass);
        g.rounded(
            vec2(0.0, -24.0),
            vec2(h.x - 5.0, h.y - 46.0),
            4.0,
            colour.shade(0.12),
        );
        g.rect(vec2(0.0, -24.0), vec2(h.x - 14.0, 2.0), 0.0, hex(0x3a6a9a));
        for k in 0..4 {
            let (x, y) = (h.x - 10.0, -h.y + 30.0 + k as f32 * 36.0);
            g.line(vec2(-x, y), vec2(x, y), 1.0, colour.shade(-0.15));
        }
    } else {
        g.rounded(vec2(0.0, h.y * 0.32), vec2(h.x - 7.0, 14.0), 6.0, glass);
        g.rounded(
            vec2(0.0, -h.y * 0.12),
            vec2(h.x - 6.0, h.y * 0.32),
            6.0,
            colour.shade(0.1),
        );
        g.rounded(vec2(0.0, -h.y * 0.55), vec2(h.x - 8.0, 10.0), 5.0, glass);
    }
    for (dx, y, half, c) in [
        (9.0, h.y - 3.0, vec2(6.0, 2.0), hex(0xe8e8d8)),
        (8.0, -h.y + 3.0, vec2(5.0, 2.0), hex(0x8a1010)),
    ] {
        g.rect(vec2(-h.x + dx, y), half, 0.0, c);
        g.rect(vec2(h.x - dx, y), half, 0.0, c);
    }
}

/// Each part is its own mesh, in the body's frame: `+x` forward, `+y` left.
pub struct RobberParts {
    pub shadow: Mesh,
    pub foot: Mesh,
    /// A knee seen from above, shown crouched.
    pub knee: Mesh,
    pub bag: Mesh,
    pub torso: Mesh,
    /// A sleeve from the shoulder along `+x`, 15 to the glove: stretched to it.
    pub arm: Mesh,
    pub glove: Mesh,
    pub crowbar: Mesh,
    pub torch: Mesh,
    pub lens: Mesh,
    pub head: Mesh,
    pub streak: Mesh,
}

const SKIN: Hex = hex(0xd8a47a);
const BEANIE: Hex = hex(0x1c1c22);
const STRIPE_LIGHT: Hex = hex(0xe6e0d2);
const BURLAP: Hex = hex(0xb08a54);

/// A door leaf from its hinge at the origin along +x.
pub fn door_leaf(kind: DoorKind) -> Mesh {
    let len = TILE - 3.0;
    let c = vec2(len * 0.5, 0.0);
    let mut g = Painter::default();
    let (edge, face) = match kind {
        DoorKind::Wood => (pal::WOOD_DARK.paint(), pal::WOOD.paint()),
        DoorKind::Steel => (pal::STEEL_DARK.paint(), pal::STEEL.paint()),
        DoorKind::Glass => (pal::BRASS.shade(-0.3).paint(), hex(0xa8d8e8).alpha(0.7)),
    };
    g.rect(c, vec2(len * 0.5, 3.0), 0.0, edge);
    g.rect(c, vec2(len * 0.5 - 1.5, 1.6), 0.0, face);
    g.circle(vec2(len - 6.0, 0.0), 2.4, pal::BRASS);
    g.circle(Vec2::ZERO, 2.2, pal::BLACK);
    g.build()
}

/// Fading, if he is not the robber looking; in his look's outfit and colour.
pub fn robber(fades: bool, look: &Look) -> RobberParts {
    let build = |f: &dyn Fn(&mut Painter)| {
        let mut g = Painter {
            fades,
            ..Painter::default()
        };
        f(&mut g);
        g.build()
    };
    RobberParts {
        shadow: build(&|g| {
            g.circle(vec2(-2.0, -2.0), 24.0, hex(0x000000).alpha(0.12));
            g.circle(vec2(-2.0, -2.0), 20.0, hex(0x000000).alpha(0.18));
        }),
        foot: build(&|g| {
            g.ellipse(Vec2::ZERO, vec2(8.5, 5.5), 0.0, hex(0x16120f));
            g.ellipse(vec2(2.0, 0.0), vec2(5.0, 3.6), 0.0, hex(0x2c2420));
        }),
        knee: build(&|g| {
            g.ellipse(Vec2::ZERO, vec2(6.5, 5.5), 0.0, hex(0x26304a));
            g.ellipse(vec2(1.0, 0.5), vec2(4.5, 3.8), 0.0, hex(0x46587e));
        }),
        bag: build(&|g| {
            g.circle(Vec2::ZERO, 12.5, BURLAP.shade(-0.4));
            g.circle(Vec2::ZERO, 11.0, BURLAP);
            g.circle(vec2(-2.5, 2.5), 5.0, BURLAP.shade(0.15));
            g.line(vec2(-4.0, -4.0), vec2(4.0, 4.0), 2.2, hex(0x2a5a2a));
            g.line(vec2(-1.0, -6.0), vec2(-1.0, 6.0), 1.2, hex(0x2a5a2a));
            g.circle(vec2(9.0, 0.0), 3.0, BURLAP.shade(-0.3));
            g.rect(vec2(6.0, 6.0), vec2(4.0, 2.2), 0.4, hex(0x7ab06a));
        }),
        torso: build(&|g| torso(g, look)),
        arm: build(&|g| sleeve(g, look)),
        glove: build(&|g| {
            g.circle(Vec2::ZERO, 4.8, hex(0x111114));
            g.circle(vec2(0.9, 0.9), 1.9, hex(0x26262c));
        }),
        crowbar: build(&|g| {
            g.line(Vec2::ZERO, vec2(26.0, 0.0), 3.0, hex(0x9a2a1e));
            g.line(vec2(26.0, 0.0), vec2(30.0, 4.0), 3.0, hex(0x9a2a1e));
            g.line(vec2(-3.0, 0.0), Vec2::ZERO, 3.0, hex(0x6a6e74));
        }),
        torch: build(&|g| {
            g.capsule(vec2(-2.0, 0.0), vec2(9.0, 0.0), 4.2, hex(0x2a2c30));
            g.rect(vec2(10.5, 0.0), vec2(2.0, 3.4), 0.0, hex(0x4a4e54));
            g.rect(vec2(12.6, 0.0), vec2(0.6, 2.8), 0.0, hex(0x8a9096));
        }),
        lens: build(&|g| {
            g.rect(
                vec2(12.6, 0.0),
                vec2(0.9, 2.8),
                0.0,
                hex(0xfff4d8).glow(3.0),
            )
        }),
        head: build(&|g| head(g, look)),
        streak: build(&|g| g.line(vec2(-16.0, 0.0), Vec2::ZERO, 2.0, hex(0xffffff).alpha(0.22))),
    }
}

/// An item's picture, in its own frame (`g.frame`): within `±half`, its long
/// side along `x`.
pub fn item(g: &mut Painter, kind: Kind, half: Vec2) {
    let (hx, hy) = (half.x, half.y);
    match kind {
        Kind::Crowbar => {
            let red = hex(0xb0301f);
            for (w, paint) in [(8.0, hex(0x3a0e08)), (5.0, red)] {
                g.line(vec2(-hx + 8.0, -3.0), vec2(hx - 16.0, -3.0), w, paint);
                g.line(vec2(-hx + 8.0, -3.0), vec2(-hx + 3.0, 4.0), w, paint);
                // The hook, round from the shaft's end.
                let c = vec2(hx - 16.0, 4.0);
                let pts: Vec<Vec2> = (0..=8)
                    .map(|i| c + Vec2::from_angle(-1.57 + 3.3 * i as f32 / 8.0) * 7.0)
                    .collect();
                for p in pts.windows(2) {
                    g.capsule(p[0], p[1], w, paint);
                }
            }
            g.line(
                vec2(-hx + 14.0, -1.8),
                vec2(hx - 22.0, -1.8),
                1.2,
                red.shade(0.3),
            );
        }
        Kind::Flashlight => {
            g.capsule(
                vec2(-hx + 8.0, 0.0),
                vec2(hx - 16.0, 0.0),
                12.0,
                hex(0x2a2c30),
            );
            g.line(
                vec2(-hx + 10.0, 3.0),
                vec2(hx - 20.0, 3.0),
                1.5,
                hex(0x4a4e56),
            );
            g.rect(vec2(hx - 11.0, 0.0), vec2(6.0, 9.5), 0.0, hex(0x4a4e54));
            g.rect(vec2(hx - 4.0, 0.0), vec2(1.6, 8.0), 0.0, hex(0xd8e2ea));
            g.rect(vec2(-4.0, 5.5), vec2(3.0, 1.6), 0.0, hex(0x8a9096));
        }
        // From the side, muzzle along `+x`: slide, frame, guard, grip.
        Kind::Pistol => {
            let steel = hex(0x34373e);
            let dark = hex(0x16171a);
            // The grip, raked back, with its stippled panel.
            let grip = [
                vec2(-24.0, 2.0),
                vec2(-10.0, 2.0),
                vec2(-14.0, -19.0),
                vec2(-28.0, -19.0),
            ];
            g.poly(&grip, dark);
            let inner = [
                vec2(-22.5, 0.0),
                vec2(-12.0, 0.0),
                vec2(-15.0, -16.5),
                vec2(-25.5, -16.5),
            ];
            g.poly(&inner, hex(0x26282d));
            for k in 0..6 {
                let y = -2.5 - 2.5 * k as f32;
                g.line(
                    vec2(-23.0 - y * 0.2, y),
                    vec2(-13.0 - y * 0.2, y),
                    0.8,
                    dark,
                );
            }
            // The frame under the slide, the trigger guard and trigger.
            g.rect(vec2(2.0, 1.5), vec2(26.0, 2.5), 0.0, dark);
            let ring: Vec<Vec2> = (0..=10)
                .map(|i| vec2(-3.0, -2.0) + Vec2::from_angle(-0.2 - 2.8 * i as f32 / 10.0) * 6.5)
                .collect();
            for p in ring.windows(2) {
                g.line(p[0], p[1], 2.0, dark);
            }
            g.line(vec2(-4.0, 0.0), vec2(-5.5, -5.0), 1.6, steel.shade(0.2));
            // The slide: its top lit, the serrations at the back, the sights.
            g.rect(vec2(2.0, 8.0), vec2(31.0, 5.5), 0.0, steel);
            g.rect(vec2(2.0, 12.5), vec2(31.0, 1.0), 0.0, steel.shade(0.3));
            for k in 0..5 {
                let x = -26.0 + 2.6 * k as f32;
                g.line(vec2(x, 4.0), vec2(x, 12.0), 0.9, dark);
            }
            g.rect(vec2(30.0, 14.5), vec2(1.2, 1.4), 0.0, dark);
            g.rect(vec2(-26.0, 14.5), vec2(2.0, 1.4), 0.0, dark);
            g.rect(vec2(34.0, 7.0), vec2(1.2, 2.0), 0.0, hex(0x0a0a0c));
        }
        Kind::Rounds => {
            for k in 0..4 {
                let x = -9.0 + 6.0 * k as f32;
                g.rect(vec2(x, -3.0), vec2(2.4, 7.0), 0.0, pal::BRASS);
                g.capsule(vec2(x, 4.0), vec2(x, 7.0), 4.4, hex(0x8a5a36));
            }
        }
        Kind::Coin => {
            for at in [vec2(-5.0, -4.0), vec2(5.0, -3.0), vec2(0.0, 5.0)] {
                coin_face(g, at, 7.5);
            }
        }
        Kind::Trap => {
            g.rounded(Vec2::ZERO, vec2(hx - 6.0, hy - 10.0), 4.0, hex(0x2b2f35));
            for side in [-1.0, 1.0] {
                g.rect(
                    vec2((hx - 10.0) * side, 0.0),
                    vec2(4.0, hy - 10.0),
                    0.0,
                    pal::GOLD,
                );
            }
            g.rect(Vec2::ZERO, vec2(hx - 18.0, hy - 16.0), 0.0, hex(0x07080a));
            g.circle(Vec2::ZERO, 2.0, hex(0xffb030));
        }
        Kind::Lockpicks => {
            for k in 0..3 {
                let y = -4.0 + 4.0 * k as f32;
                g.line(vec2(-6.0, y), vec2(hx - 4.0, y + 3.0), 1.6, hex(0xc8ccd2));
            }
            g.rounded(vec2(-6.0, 0.0), vec2(9.0, hy - 6.0), 3.0, hex(0x6a3424));
            g.line(
                vec2(-6.0, -hy + 7.0),
                vec2(-6.0, hy - 7.0),
                1.4,
                hex(0x4a2418),
            );
        }
    }
}

fn coin_face(g: &mut Painter, at: Vec2, r: f32) {
    g.circle(at, r, pal::GOLD.shade(-0.4));
    g.circle(at, r * 0.8, pal::GOLD);
    g.circle(at + vec2(-0.25, 0.25) * r, r * 0.3, pal::GOLD.shade(0.4));
}

/// A coin, in his hand, in the air or on the floor.
pub fn coin() -> Mesh {
    let mut g = Painter::default();
    coin_face(&mut g, Vec2::ZERO, 3.4);
    g.build()
}

/// A pistol from above, held: its grip at the origin, its muzzle 16 along
/// `+x`. Slide, its lit top edge, the serrations at its back, the sight.
pub fn pistol() -> Mesh {
    let mut g = Painter::default();
    let steel = hex(0x3a3e46);
    g.rounded(vec2(6.0, 0.0), vec2(10.0, 3.0), 1.2, hex(0x111215));
    g.rounded(vec2(6.5, 0.0), vec2(9.0, 2.1), 0.8, steel);
    g.line(vec2(-1.5, 0.9), vec2(15.0, 0.9), 0.8, steel.shade(0.35));
    for k in 0..3 {
        let x = -1.5 + 1.4 * k as f32;
        g.line(vec2(x, -2.0), vec2(x, 2.0), 0.5, hex(0x1c1d21));
    }
    g.circle(vec2(14.0, 0.0), 0.6, hex(0xd8dcd2));
    g.circle(vec2(16.0, 0.0), 1.1, hex(0x0a0a0c));
    g.build()
}

/// The reticle on the pointer, in `paint`: a ring and four ticks.
pub fn reticle(paint: Paint) -> Mesh {
    let mut g = Painter::default();
    for i in 0..24 {
        let a = |i: usize| Vec2::from_angle(i as f32 / 24.0 * TAU) * 8.0;
        g.line(a(i), a(i + 1), 1.5, paint);
    }
    for k in 0..4 {
        let d = Vec2::from_angle(k as f32 * TAU / 4.0);
        g.line(d * 11.0, d * 16.0, 1.5, paint);
    }
    g.circle(Vec2::ZERO, 1.2, paint);
    g.build()
}

/// A shot's streak, 0 to 1 along `+x` (scaled to its length), and its flash.
pub fn tracer() -> Mesh {
    let mut g = Painter::default();
    g.line(Vec2::ZERO, vec2(1.0, 0.0), 1.6, hex(0xffd890).glow(3.0));
    g.build()
}

pub fn flash() -> Mesh {
    let mut g = Painter::default();
    let mut halo = hex(0xffb050).glow(2.5);
    halo.rgba[3] = 0.7;
    g.circle(Vec2::ZERO, 7.0, halo);
    g.circle(Vec2::ZERO, 3.5, hex(0xfff0c0).glow(4.0));
    g.build()
}

/// A thin ring of radius `r`: how far a coin goes.
pub fn ring(r: f32) -> Mesh {
    let mut g = Painter::default();
    let n = 96;
    let paint = hex(0xe8c860).alpha(0.3);
    for i in 0..n {
        let a = |i: usize| Vec2::from_angle(i as f32 / n as f32 * TAU) * r;
        g.line(a(i), a(i + 1), 2.0, paint);
    }
    g.build()
}

/// Where a coin would land: a small gold ring and dot.
pub fn target() -> Mesh {
    let mut g = Painter::default();
    let paint = hex(0xe8c860).alpha(0.8);
    for i in 0..24 {
        let a = |i: usize| Vec2::from_angle(i as f32 / 24.0 * TAU) * 9.0;
        g.line(a(i), a(i + 1), 2.0, paint);
    }
    g.circle(Vec2::ZERO, 2.0, paint);
    g.build()
}

const PALE: Hex = hex(0xc9d3d6);
const BONE: Hex = hex(0x6f7d86);
const HOLLOW: Hex = hex(0x040405);
const HAIR: Hex = hex(0x0b0b0e);
const SHROUD: Hex = hex(0x9aa6ad);

pub fn noise1(x: f32) -> f32 {
    ((x * 12.9898).sin() * 43758.547).fract().abs()
}

/// The wraith at `t` seconds, facing `+x`, crawling: a mass of wet black hair
/// over its back, a pale face thrown back to stare up out of the screen
/// through it, bony arms too long reaching ahead, torn shroud trailing. It
/// sways, and every few seconds it jerks and flickers. Dimly lit from within
/// (it reads in the dark) and fading out of sight like people.
pub fn wraith(t: f32) -> Mesh {
    let sway = |speed: f32, seed: f32| {
        (t * speed + seed).sin() * 0.6 + (t * speed * 1.7 + seed * 1.3).sin() * 0.4
    };
    // A glitch: 0.25 s at a random moment of every 2.5 s, jumping each 20th.
    let slot = (t / 2.5).floor();
    let start = slot * 2.5 + 0.4 + 1.7 * noise1(slot);
    let glitch = t >= start && t < start + 0.25;
    let tick = (t * 20.0).floor();
    let jolt = if glitch {
        vec2(noise1(tick) - 0.5, noise1(tick + 0.5) - 0.5) * 10.0
    } else {
        Vec2::ZERO
    };
    let k = if glitch && noise1(tick + 0.25) < 0.5 {
        0.3
    } else {
        1.0
    };
    let mut g = Painter {
        fades: true,
        ..Painter::default()
    };
    g.frame.origin = jolt;
    let lit = |h: Hex, power: f32, a: f32| {
        let mut p = h.glow(power);
        p.rgba[3] = a * k;
        p
    };
    let flat = |h: Hex, a: f32| h.alpha(a * k);

    // The cold patch it crawls in.
    for (r, a) in [(50.0, 0.10), (38.0, 0.12), (26.0, 0.16)] {
        g.circle(vec2(-4.0, 0.0), r, flat(hex(0x000000), a));
    }
    // Shroud tatters trailing behind, narrowing and thinning to nothing.
    for i in 0..5 {
        let y0 = -10.0 + 5.0 * i as f32;
        let len = 40.0 + 18.0 * noise1(i as f32 + 0.3);
        let mut prev = vec2(-10.0, y0);
        for s in 1..=6 {
            let f = s as f32 / 6.0;
            let wave = sway(1.4, i as f32 * 0.9 + f * 2.0) * 7.0 * f;
            let p = vec2(-10.0 - len * f, y0 * (1.0 + 0.7 * f) + wave);
            let paint = lit(SHROUD, 0.2, 0.32 * (1.0 - f) + 0.04);
            g.line(prev, p, 10.0 * (1.0 - 0.85 * f), paint);
            prev = p;
        }
    }
    // Arms too long, elbows out like a spider's, one reaching further; the
    // fingers twitch.
    for (side, reach) in [(-1.0, 1.0), (1.0, 0.8)] {
        let shoulder = vec2(2.0, 10.0 * side);
        let elbow = vec2(14.0 + 3.0 * sway(0.7, side), 21.0 * side);
        let wrist = vec2(
            16.0 + 26.0 * reach + 3.0 * sway(0.9, side + 1.0),
            11.0 * side,
        );
        for (a, b, w) in [(shoulder, elbow, 3.4), (elbow, wrist, 2.6)] {
            g.capsule(a, b, w + 1.0, lit(BONE, 0.25, 0.8));
            g.capsule(a, b, w, lit(PALE, 0.32, 0.8));
            let n = (b - a).perp().normalize() * w * 0.25 * side;
            g.line(a + n, b + n, 0.7, lit(BONE, 0.25, 0.7));
        }
        let dir = (wrist - elbow).normalize();
        let palm = wrist + dir * 3.5;
        g.ellipse(palm, vec2(3.6, 2.8), dir.to_angle(), lit(PALE, 0.35, 0.85));
        for f in 0..4 {
            let spread = (f as f32 - 1.5) * 0.42 * side;
            let curl = (0.45 + 0.3 * sway(6.0 + f as f32, f as f32 * 2.1)) * -side;
            let d1 = dir.rotate(Vec2::from_angle(spread));
            let d2 = d1.rotate(Vec2::from_angle(curl));
            let base = palm + d1 * 2.5;
            let knuckle = base + d1 * 8.5;
            let tip = knuckle + d2 * 7.0;
            g.capsule(base, knuckle, 1.5, lit(PALE, 0.35, 0.85));
            g.capsule(knuckle, tip, 1.1, lit(PALE, 0.35, 0.85));
            g.circle(tip, 0.8, flat(HOLLOW, 0.9));
        }
    }
    // Bony shoulders, mostly under the hair.
    g.ellipse(Vec2::ZERO, vec2(8.0, 12.5), 0.0, lit(BONE, 0.25, 0.85));
    g.ellipse(vec2(0.5, 0.0), vec2(7.0, 11.4), 0.0, lit(PALE, 0.3, 0.85));
    // The head, swaying, snapping aside in a glitch.
    let snap = if glitch {
        (noise1(tick + 0.75) - 0.5) * 1.2
    } else {
        0.0
    };
    let tilt = sway(0.5, 2.0) * 0.12 + snap;
    let face = |p: Vec2| vec2(13.0, 0.0) + Vec2::from_angle(tilt).rotate(p);
    // Hair: a wet mass over the back, clumps flowing back over the shoulders.
    g.ellipse(
        face(vec2(-12.0, 0.0)),
        vec2(17.0, 13.0),
        tilt,
        flat(HAIR, 0.97),
    );
    for i in 0..24 {
        let a = PI + (i as f32 / 23.0 - 0.5) * 1.3;
        let len = 34.0 + 22.0 * noise1(i as f32 + 0.7);
        let root = face(Vec2::from_angle(a) * 7.0);
        let wet = if i % 5 == 2 { HAIR.shade(0.12) } else { HAIR };
        let mut prev = root;
        for s in 1..=7 {
            let f = s as f32 / 7.0;
            let out = Vec2::from_angle(a) * vec2(1.0, 0.9) * len * f;
            let wave = (f * 9.0 + i as f32).sin() * 2.0 + sway(0.9, i as f32 * 1.3 + f) * 3.5;
            let p = root + out + vec2(0.0, wave * f);
            g.line(prev, p, 5.5 * (1.0 - 0.7 * f), flat(wet, 0.95 - 0.45 * f));
            prev = p;
        }
    }
    // The face up through the hair, forehead back (-x), chin ahead: sunken
    // round the eyes, the jaw hanging open in a long scream.
    g.ellipse(
        face(vec2(0.5, 0.0)),
        vec2(12.6, 9.0),
        tilt,
        lit(BONE, 0.3, 0.95),
    );
    g.ellipse(
        face(vec2(0.5, 0.0)),
        vec2(11.8, 8.2),
        tilt,
        lit(PALE, 0.45, 0.95),
    );
    g.ellipse(
        face(vec2(-2.0, 0.0)),
        vec2(3.8, 7.4),
        tilt,
        lit(BONE, 0.3, 0.7),
    );
    for side in [-1.0, 1.0] {
        let cheek = face(vec2(3.0, 5.2 * side));
        g.ellipse(
            cheek,
            vec2(4.2, 1.6),
            tilt + 0.15 * side,
            lit(BONE, 0.3, 0.6),
        );
        let eye = vec2(-2.4, 3.4 * side);
        g.ellipse(
            face(eye),
            vec2(2.7, 1.6),
            tilt - 0.35 * side,
            flat(HOLLOW, 1.0),
        );
        if !glitch {
            let drift = vec2(sway(0.4, side), sway(0.5, side + 3.0)) * 0.4;
            g.circle(face(eye + drift), 0.45, lit(hex(0xdfe9f2), 1.6, 0.9));
        }
        g.circle(face(vec2(1.2, 0.7 * side)), 0.45, flat(HOLLOW, 0.9));
    }
    let open = 0.5 + 0.5 * (t * 0.7).sin();
    let mouth = face(vec2(6.0 + 1.0 * open, 0.0));
    g.ellipse(
        mouth,
        vec2(2.9 + 2.6 * open, 2.5),
        tilt,
        lit(BONE, 0.2, 0.9),
    );
    g.ellipse(mouth, vec2(2.6 + 2.5 * open, 2.1), tilt, flat(HOLLOW, 1.0));
    // Hair over the forehead and in wet strands down the sides of the face.
    g.ellipse(
        face(vec2(-11.0, 0.0)),
        vec2(3.6, 8.6),
        tilt,
        flat(HAIR, 0.97),
    );
    for side in [-1.0, 1.0] {
        for s in 0..4 {
            let y = (5.0 + 1.3 * s as f32) * side;
            let end = vec2(
                3.0 + 2.8 * s as f32,
                y + (2.4 + sway(0.8, s as f32 + side) * 0.8) * side,
            );
            g.line(face(vec2(-10.0, y * 0.8)), face(end), 1.8, flat(HAIR, 0.9));
        }
    }
    g.build()
}

/// A bare footprint the wraith leaves, toes along `+x`, the left one if
/// `left`: a faint cold glow, to be seen in the dark, at `strength` (0 to 1).
pub fn footprint(left: bool, strength: f32) -> Mesh {
    let mut g = Painter {
        fades: true,
        ..Painter::default()
    };
    let side = if left { 1.0 } else { -1.0 };
    let mut ink = hex(0x9fe8ff).glow(0.5);
    ink.rgba[3] = 0.55 * strength;
    g.ellipse(vec2(2.0, 0.6 * side), vec2(5.5, 3.0), 0.08 * side, ink);
    g.ellipse(vec2(-5.5, 0.0), vec2(3.0, 2.4), 0.0, ink);
    for (i, r) in [1.5, 1.1, 1.0, 0.9, 0.8].into_iter().enumerate() {
        let y = (2.6 - 1.4 * i as f32) * side;
        g.circle(vec2(8.6 - 0.5 * i as f32 * i as f32 * 0.3, y), r, ink);
    }
    g.build()
}

/// A note on the floor: a sheet of paper, a corner turned, scrawled on in
/// ink; gone out of sight like people, so it is found by looking.
pub fn note() -> Mesh {
    let mut g = Painter {
        fades: true,
        ..Painter::default()
    };
    g.rect(
        vec2(1.0, -1.5),
        vec2(7.5, 9.5),
        0.0,
        hex(0x000000).alpha(0.25),
    );
    g.poly(
        &[
            vec2(-7.0, -9.0),
            vec2(7.0, -9.0),
            vec2(7.0, 5.5),
            vec2(3.5, 9.0),
            vec2(-7.0, 9.0),
        ],
        pal::PAPER,
    );
    g.poly(
        &[vec2(3.5, 5.5), vec2(7.0, 5.5), vec2(3.5, 9.0)],
        pal::PAPER.shade(-0.18),
    );
    let ink = hex(0x2a2430);
    g.line(vec2(-3.5, 4.0), vec2(0.5, -4.5), 2.0, ink);
    g.line(vec2(0.5, -4.5), vec2(3.0, 3.0), 2.0, ink);
    g.line(vec2(-4.5, -6.5), vec2(4.5, -6.0), 1.0, ink.shade(0.3));
    g.build()
}

/// The ghost trap from above: a steel box, hazard stripes at its ends. Set,
/// its jaws stand open and an amber light shows it armed; sprung, they are
/// shut with the wraith's cold light at the seam.
pub fn trap(sprung: bool) -> Mesh {
    let mut g = Painter::default();
    g.rounded(
        vec2(2.0, -3.0),
        vec2(17.0, 12.0),
        4.0,
        hex(0x000000).alpha(0.3),
    );
    g.rounded(Vec2::ZERO, vec2(16.0, 11.0), 3.0, hex(0x2b2f35));
    for side in [-1.0, 1.0] {
        let x = 13.0 * side;
        g.rect(vec2(x, 0.0), vec2(3.0, 11.0), 0.0, hex(0x1a1a1c));
        for k in -2..=2 {
            let y = k as f32 * 4.4;
            g.rect(vec2(x, y), vec2(3.0, 1.1), 0.6, pal::GOLD);
        }
    }
    if sprung {
        g.rect(vec2(0.0, 4.5), vec2(9.5, 4.3), 0.0, pal::STEEL);
        g.rect(vec2(0.0, -4.5), vec2(9.5, 4.3), 0.0, pal::STEEL_DARK);
        g.rect(Vec2::ZERO, vec2(9.5, 0.6), 0.0, hex(0x9fe8ff).glow(1.6));
        g.circle(vec2(0.0, 9.0), 1.2, hex(0xff4030).glow(2.5));
    } else {
        g.rect(Vec2::ZERO, vec2(9.5, 8.6), 0.0, hex(0x07080a));
        for side in [-1.0, 1.0] {
            g.rect(
                vec2(0.0, 11.5 * side),
                vec2(9.5, 2.2),
                0.0,
                pal::STEEL.shade(-0.1),
            );
        }
        g.circle(vec2(0.0, 0.0), 1.2, hex(0xffb030).glow(2.5));
    }
    g.build()
}

const JACKET: Hex = hex(0x23252b);
const WHITE: Hex = hex(0xe8e6e0);

/// His top from above, facing `+x`, `r` its half size.
fn torso(g: &mut Painter, look: &Look) {
    let c = look.colour();
    let r = vec2(11.5, 17.0);
    let body = |g: &mut Painter, cloth: Hex| {
        g.ellipse(Vec2::ZERO, r + Vec2::splat(1.2), 0.0, cloth.shade(-0.45));
        g.ellipse(Vec2::ZERO, r, 0.0, cloth);
    };
    match look.outfit {
        Outfit::Stripes => striped_ellipse(g, r, c),
        Outfit::Hoodie => {
            body(g, c);
            // The hood down his back, a seam over the pouch.
            g.ellipse(vec2(-6.0, 0.0), vec2(4.5, 9.0), 0.0, c.shade(-0.2));
            g.line(vec2(3.0, -7.0), vec2(3.0, 7.0), 1.2, c.shade(-0.3));
        }
        Outfit::Suit => {
            body(g, JACKET);
            // The shirt's V at his collar, his tie in his colour.
            g.poly(
                &[vec2(r.x - 0.5, -5.0), vec2(r.x - 0.5, 5.0), vec2(2.5, 0.0)],
                WHITE,
            );
            g.line(vec2(4.5, 0.0), vec2(r.x - 0.8, 0.0), 2.6, c);
            g.line(
                vec2(-r.x + 1.5, 0.0),
                vec2(-4.0, 0.0),
                0.8,
                JACKET.shade(-0.3),
            );
        }
        Outfit::Tracksuit => {
            body(g, c);
            // White stripes over his shoulders, the zip at his front.
            for side in [-1.0, 1.0] {
                g.line(vec2(-5.5, 13.5 * side), vec2(5.5, 13.5 * side), 1.6, WHITE);
            }
            g.line(
                vec2(r.x - 5.0, 0.0),
                vec2(r.x - 0.6, 0.0),
                1.0,
                c.shade(0.45),
            );
        }
    }
}

/// His sleeves' cloth, and the band round them.
fn sleeve_cloth(look: &Look) -> Hex {
    match look.outfit {
        Outfit::Suit => JACKET,
        _ => look.colour(),
    }
}

/// A sleeve from the shoulder along `+x`, 15 to the glove.
fn sleeve(g: &mut Painter, look: &Look) {
    let band = match look.outfit {
        Outfit::Stripes => STRIPE_LIGHT,
        Outfit::Hoodie => look.colour().shade(-0.25),
        Outfit::Suit => JACKET.shade(0.15),
        Outfit::Tracksuit => WHITE,
    };
    g.capsule(Vec2::ZERO, vec2(14.0, 0.0), 8.5, sleeve_cloth(look));
    g.line(vec2(4.0, 0.0), vec2(7.0, 0.0), 8.0, band);
}

/// His head from above, his face to `+x`: a beanie, a hood, slicked hair and
/// a mask, or a cap, by his outfit.
fn head(g: &mut Painter, look: &Look) {
    let c = look.colour();
    let mask = hex(0x0e0e10);
    g.circle(Vec2::ZERO, 10.5, SKIN.shade(-0.25));
    g.circle(vec2(0.5, 0.0), 9.8, SKIN);
    match look.outfit {
        Outfit::Stripes => {
            g.ellipse(vec2(6.0, 0.0), vec2(3.5, 8.6), 0.0, mask);
            g.circle(vec2(-1.8, 0.0), 9.6, BEANIE);
            g.ellipse(vec2(3.6, 0.0), vec2(2.0, 9.0), 0.0, BEANIE.shade(0.15));
            g.line(vec2(-8.0, 0.0), vec2(2.0, 0.0), 1.0, BEANIE.shade(0.1));
            g.circle(vec2(-4.0, 0.0), 3.8, c.shade(-0.2));
            g.circle(vec2(-4.6, 0.6), 1.8, c.shade(0.15));
        }
        Outfit::Hoodie => {
            g.circle(vec2(-1.0, 0.0), 11.2, c.shade(-0.2));
            g.ellipse(vec2(6.0, 0.0), vec2(3.6, 7.6), 0.0, mask);
            g.ellipse(vec2(3.2, 0.0), vec2(1.6, 9.6), 0.0, c.shade(-0.35));
        }
        Outfit::Suit => {
            g.ellipse(vec2(6.0, 0.0), vec2(3.0, 8.2), 0.0, mask);
            g.circle(vec2(-2.0, 0.0), 9.0, hex(0x2a1d14));
            g.line(vec2(-9.0, 2.0), vec2(1.5, 2.0), 0.8, hex(0x1a120c));
        }
        Outfit::Tracksuit => {
            g.ellipse(vec2(6.0, 0.0), vec2(3.4, 8.4), 0.0, mask);
            g.circle(vec2(-1.0, 0.0), 9.8, c);
            g.ellipse(vec2(8.5, 0.0), vec2(4.5, 7.8), 0.0, c.shade(-0.3));
            g.circle(vec2(-1.0, 0.0), 1.4, c.shade(0.3));
        }
    }
}

/// The robber standing, seen from above, his face up the screen, `k` times
/// as big: for the lobby, in one mesh.
pub fn portrait(look: &Look, k: f32) -> Mesh {
    let mut g = Painter::default();
    // From his bag behind to his face, centred.
    let base = Frame {
        origin: Vec2::Y * 4.5 * k,
        x: Vec2::Y * k,
        y: Vec2::NEG_X * k,
    };
    let at = |p: Vec2, angle: f32| {
        let (s, c) = angle.sin_cos();
        Frame {
            origin: base.at(p),
            x: base.x * c + base.y * s,
            y: base.y * c - base.x * s,
        }
    };
    for side in [-1.0, 1.0] {
        g.frame = at(vec2(-1.0, 8.0 * side), 0.0);
        g.ellipse(Vec2::ZERO, vec2(8.5, 5.5), 0.0, hex(0x16120f));
        g.ellipse(vec2(2.0, 0.0), vec2(5.0, 3.6), 0.0, hex(0x2c2420));
        // His arms hang at his sides.
        let (shoulder, hand) = (vec2(1.0, 14.5 * side), vec2(-3.0, 21.0 * side));
        g.frame = at(shoulder, (hand - shoulder).to_angle());
        g.capsule(
            Vec2::ZERO,
            vec2(hand.distance(shoulder), 0.0),
            8.5,
            sleeve_cloth(look),
        );
        g.frame = at(hand, 0.0);
        g.circle(Vec2::ZERO, 4.8, hex(0x111114));
    }
    g.frame = base;
    torso(&mut g, look);
    g.frame = at(vec2(-11.0, 0.0), 0.0);
    g.circle(Vec2::ZERO, 11.0, BURLAP.shade(-0.4));
    g.circle(Vec2::ZERO, 9.6, BURLAP);
    g.frame = at(vec2(2.0, 0.0), 0.0);
    head(&mut g, look);
    g.build()
}

/// An ellipse in seven vertical bands, dark (in `c`) and light.
fn striped_ellipse(g: &mut Painter, r: Vec2, c: Hex) {
    g.ellipse(Vec2::ZERO, r + Vec2::splat(1.2), 0.0, c.shade(-0.5));
    for b in 0..7 {
        let x0 = -r.x + 2.0 * r.x * b as f32 / 7.0;
        let x1 = -r.x + 2.0 * r.x * (b + 1) as f32 / 7.0;
        let mut pts = Vec::new();
        for i in 0..=6 {
            let x = x0 + (x1 - x0) * i as f32 / 6.0;
            pts.push(vec2(x, r.y * (1.0 - (x / r.x).powi(2)).max(0.0).sqrt()));
        }
        for i in (0..=6).rev() {
            pts.push(vec2(pts[i].x, -pts[i].y));
        }
        g.poly(&pts, [c, STRIPE_LIGHT][b % 2]);
    }
    g.ellipse(vec2(r.x - 3.0, 0.0), vec2(2.5, 6.0), 0.0, c.shade(-0.3));
}
