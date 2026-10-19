//! `seen` from world.wgsl on the CPU, over the same grids, for whoever looks
//! at the robber.

use crate::map::{Map, Wall, tile_of};
use crate::movement::TILE;
use bevy::prelude::*;
use std::collections::BinaryHeap;

/// The robber's sight in tiles, as in world.wgsl: in a lit room or outside, and in a dark one.
pub const VIEW: f32 = 6.0;
pub const DARK: f32 = 3.75;
const EYE_SPREAD: f32 = 7.0;
// The flashlight, as in world.wgsl: its lens ahead of him, its cone's inner and outer half angles.
const LENS_AHEAD: f32 = 16.0;
const CONE_IN: f32 = 0.30;
const CONE_OUT: f32 = 0.52;
const STOP_SOFT: f32 = 3.0;
/// The tiles of reach a noise loses to each wall or shut door it passes.
pub const THROUGH_WALL: f32 = 4.0;

#[derive(Resource)]
pub struct Sight {
    w: i32,
    h: i32,
    tall: Vec<[u8; 4]>,
    low: Vec<[u8; 4]>,
    low_ids: Vec<u8>,
    glass: Vec<bool>,
    /// Any wall, windows too: what a noise loses `THROUGH_WALL` to.
    walls: Vec<bool>,
    lit: Vec<bool>,
}

#[derive(Default)]
struct Run {
    opaque: f32,
    glass: f32,
    /// How far the end lies past the first half-height object crossed.
    past: Option<f32>,
}

impl Sight {
    pub fn new(map: &Map) -> Self {
        let (tall, low) = (map.occluders(false), map.occluders(true));
        Sight {
            w: map.w,
            h: map.h,
            tall: tall.boxes,
            low: low.boxes,
            low_ids: low.objects,
            glass: map.tiles.iter().map(|t| t.wall == Wall::Glass).collect(),
            walls: map.tiles.iter().map(|t| t.wall != Wall::None).collect(),
            lit: map.lit(),
        }
    }

    pub fn index(&self, p: Vec2) -> Option<usize> {
        let (x, y) = tile_of(p);
        (x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| (y * self.w + x) as usize)
    }

    /// Which tiles are lit now, the lamps switched as they are.
    pub fn set_lit(&mut self, lit: Vec<bool>) {
        self.lit = lit;
    }

    pub fn lit_at(&self, p: Vec2) -> bool {
        self.index(p).is_none_or(|i| self.lit[i])
    }

    /// What is left of a noise `loud` tiles strong from `at`, tile by tile (0
    /// where it has died out): it spreads across the floor a tile for a tile,
    /// round furniture, and loses `THROUGH_WALL` more to each wall or shut
    /// door it passes.
    pub fn carry(&self, at: Vec2, loud: f32, doors: &[u8]) -> Vec<f32> {
        let mut left = vec![0.0f32; self.walls.len()];
        let Some(start) = self.index(at) else {
            return left;
        };
        let blocked = |i: usize| self.walls[i] || doors.get(i * 4).is_some_and(|&s| s > 127);
        let key = |v: f32| (v * 1000.0) as u32;
        let mut heap = BinaryHeap::from([(key(loud), start)]);
        left[start] = loud;
        while let Some((k, i)) = heap.pop() {
            if k < key(left[i]) {
                continue;
            }
            let (x, y) = (i as i32 % self.w, i as i32 / self.w);
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
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= self.w || ny >= self.h {
                    continue;
                }
                let j = (ny * self.w + nx) as usize;
                let step = if dx != 0 && dy != 0 { 1.41 } else { 1.0 };
                let there = left[i] - step - if blocked(j) { THROUGH_WALL } else { 0.0 };
                if there > left[j] {
                    left[j] = there;
                    heap.push((key(there), j));
                }
            }
        }
        left
    }

    /// Whether a shot stops at `p`: a wall, a window, a tall thing or a
    /// mostly shut door.
    pub fn stops(&self, p: Vec2, doors: &[u8]) -> bool {
        let Some(i) = self.index(p) else {
            return true;
        };
        inside(self.tall[i], (p / TILE).fract() * 255.0)
            || self.glass[i]
            || doors.get(i * 4).is_some_and(|&s| s > 127)
    }

    /// How far the robber sees towards `at`, in tiles, as `seen` in
    /// world.wgsl: by the light there, and in his flashlight's beam as far as in a lit room.
    pub fn robber_reach(&self, eye: Vec2, heading: f32, torch: bool, at: Vec2) -> f32 {
        let ahead = Vec2::from_angle(heading);
        let off = (at - (eye + ahead * LENS_AHEAD)).normalize_or_zero();
        let beam = if torch {
            smoothstep(CONE_OUT.cos(), CONE_IN.cos(), off.dot(ahead))
        } else {
            0.0
        };
        let base = if self.lit_at(at) { VIEW } else { DARK };
        base + (VIEW - base) * beam
    }

    /// How clearly `eye` sees `at`, 0 to 1, within `reach` tiles. Crouched
    /// (`low`), a half-height object hides what lies more than 2 tiles past it.
    /// `doors` is the doors texture's bytes.
    pub fn seen(&self, eye: Vec2, at: Vec2, reach: f32, low: bool, doors: &[u8]) -> f32 {
        let off = at - eye;
        let dist = off.length();
        if dist < 20.0 {
            return 1.0;
        }
        let reach = reach * TILE;
        let ranged = 1.0 - smoothstep(reach - 0.8 * TILE, reach, dist);
        if ranged <= 0.0 {
            return 0.0;
        }
        let side = off.perp() / dist * EYE_SPREAD;
        let clear: f32 = [-1.0, 0.0, 1.0]
            .iter()
            .map(|k| {
                let run = self.trace(eye + side * *k, at, low, doors);
                let over = run
                    .past
                    .map_or(1.0, |p| 1.0 - smoothstep(1.5 * TILE, 2.0 * TILE, p));
                let glass = if run.glass > 1.0 { 0.85 } else { 1.0 };
                (1.0 - smoothstep(0.0, STOP_SOFT, run.opaque)) * glass * over
            })
            .sum();
        clear / 3.0 * ranged
    }

    /// Sampled a world unit at a time: 3 units through a box stop a ray, so
    /// that is fine enough.
    fn trace(&self, a: Vec2, b: Vec2, low: bool, doors: &[u8]) -> Run {
        let len = a.distance(b);
        let n = len.ceil().max(1.0);
        let step = len / n;
        let mut run = Run::default();
        let mut cover: Option<(u8, f32)> = None;
        for k in 0..n as usize {
            let t = (k as f32 + 0.5) * step;
            let p = a.lerp(b, t / len);
            let Some(i) = self.index(p) else {
                continue;
            };
            let f = (p / TILE).fract() * 255.0;
            if inside(self.tall[i], f) {
                run.opaque += step;
            }
            if self.glass[i] {
                run.glass += step;
            }
            // A shut door: a slab across its doorway, as in world.wgsl's trace.
            if let Some(&[shut, along_x, glass, _]) = doors.get(i * 4..i * 4 + 4)
                && shut > 0
            {
                let slab = if along_x > 127 {
                    [0, 115, 255, 140]
                } else {
                    [115, 0, 140, 255]
                };
                if inside(slab, f) {
                    let through = step * shut as f32 / 255.0;
                    if glass > 127 {
                        run.glass += through;
                    } else {
                        run.opaque += through;
                    }
                }
            }
            let id = self.low_ids[i];
            if low && inside(self.low[i], f) && cover.is_none_or(|(c, _)| c == id) {
                cover = Some((id, t));
            }
        }
        run.past = cover.map(|(_, t)| len - t);
        run
    }
}

/// Whether a point, in 0..255 of its tile, lies in a box of the same.
fn inside(b: [u8; 4], f: Vec2) -> bool {
    let [x0, y0, x1, y1] = b.map(f32::from);
    x1 > x0 && f.x >= x0 && f.x <= x1 && f.y >= y0 && f.y <= y1
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Floor, NO_ROOM, Tile, tile_centre};

    /// Three rows of 20 tiles, a brick wall across at x = 10.
    fn row() -> Sight {
        let tile = |wall| Tile {
            floor: Floor::Concrete,
            wall,
            tint: 0,
            deco: 0,
            room: NO_ROOM,
            indoor: false,
        };
        let tiles = (0..60)
            .map(|i| {
                tile(if i % 20 == 10 {
                    Wall::Brick
                } else {
                    Wall::None
                })
            })
            .collect();
        Sight::new(&Map {
            w: 20,
            h: 3,
            tiles,
            rooms: Vec::new(),
            props: Vec::new(),
            lights: Vec::new(),
            spawn: Vec2::ZERO,
            building: (0, 0, 0, 0),
        })
    }

    #[test]
    fn a_car_hides_the_robber_crouched_behind_it() {
        let map = crate::map::generate(6);
        let sight = Sight::new(&map);
        // A car with nothing else in the way: seen over standing, hidden crouched.
        let (robber, watcher) = map
            .props
            .iter()
            .filter(|p| p.kind == crate::map::PropKind::Car)
            .map(|p| {
                let car = p.body;
                let robber = vec2(car.centre().x, car.max.y + 24.0);
                (robber, vec2(car.centre().x, car.min.y - 4.0 * TILE))
            })
            .find(|&(robber, watcher)| sight.seen(robber, watcher, 12.0, false, &[]) > 0.5)
            .expect("a car to look over");
        assert!(sight.seen(robber, watcher, 12.0, true, &[]) < 0.5);
    }

    #[test]
    fn sight_reaches_its_range_and_stops_at_walls() {
        let seen = |s: &Sight, from: i32, to: i32, reach| {
            s.seen(tile_centre(from, 1), tile_centre(to, 1), reach, false, &[]) > 0.5
        };
        let s = row();
        assert!(seen(&s, 0, 9, 10.0));
        assert!(!seen(&s, 0, 9, 8.0));
        assert!(seen(&s, 11, 19, 9.0));
        assert!(!seen(&s, 11, 19, 5.0));
        assert!(!seen(&s, 8, 12, 10.0));
    }
}
