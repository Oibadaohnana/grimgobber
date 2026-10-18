//! Paths over the tiles a body fits on (`Map::free`), for whatever walks
//! the bank on its own.

use crate::map::{Map, tile_centre, tile_of};
use bevy::prelude::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// The tiles a body fits on, for paths.
#[derive(Resource)]
pub struct Walkable {
    w: i32,
    free: Vec<bool>,
}

impl Walkable {
    pub fn new(map: &Map) -> Self {
        Walkable {
            w: map.w,
            free: map.free(),
        }
    }

    /// Tile centres from `from` to the reachable free tile nearest `to`, the
    /// next last. Diagonals only between two free sides.
    pub fn path(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let (w, h) = (self.w, self.free.len() as i32 / self.w);
        let ok =
            |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && self.free[(y * w + x) as usize];
        let (sx, sy) = tile_of(from);
        if sx < 0 || sy < 0 || sx >= w || sy >= h {
            return Vec::new();
        }
        let start = (sy * w + sx) as usize;
        let mut cost = vec![u32::MAX; self.free.len()];
        let mut prev = vec![start; self.free.len()];
        let mut heap = BinaryHeap::from([Reverse((0, start))]);
        cost[start] = 0;
        while let Some(Reverse((c, i))) = heap.pop() {
            if c > cost[i] {
                continue;
            }
            let (x, y) = (i as i32 % w, i as i32 / w);
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
                let diagonal = dx != 0 && dy != 0;
                if !ok(x + dx, y + dy) || (diagonal && !(ok(x + dx, y) && ok(x, y + dy))) {
                    continue;
                }
                let n = ((y + dy) * w + x + dx) as usize;
                let nc = c + if diagonal { 7 } else { 5 };
                if nc < cost[n] {
                    cost[n] = nc;
                    prev[n] = i;
                    heap.push(Reverse((nc, n)));
                }
            }
        }
        let centre = |i: usize| tile_centre(i as i32 % w, i as i32 / w);
        let near = |i: usize| centre(i).distance_squared(to);
        let Some(mut i) = (0..cost.len())
            .filter(|&i| cost[i] < u32::MAX)
            .min_by(|&a, &b| near(a).total_cmp(&near(b)))
        else {
            return Vec::new();
        };
        let mut path = Vec::new();
        while i != start {
            path.push(centre(i));
            i = prev[i];
        }
        path
    }

    pub fn is_free(&self, x: i32, y: i32) -> bool {
        let i = (y * self.w + x) as usize;
        x >= 0 && y >= 0 && x < self.w && i < self.free.len() && self.free[i]
    }

    /// `path`, ending on `to` itself.
    pub fn path_to(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let mut path = self.path(from, to);
        match path.first() {
            Some(&end) if tile_of(end) == tile_of(to) => path[0] = to,
            _ => path.insert(0, to),
        }
        path
    }
}
