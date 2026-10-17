//! The robber's gear, DayZ fashion: items take up blocks of cells in one
//! grid, one is in his hands, and quick slots 1–4 take a bound item into his
//! hands or put it away.

use bevy::math::{IVec2, ivec2};
use bevy::prelude::Component;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Crowbar,
    Flashlight,
    Lockpicks,
    Coin,
    /// Its `count` is the rounds in its magazine.
    Pistol,
    Rounds,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Crowbar => "Crowbar",
            Kind::Flashlight => "Flashlight",
            Kind::Lockpicks => "Lockpicks",
            Kind::Coin => "Coins",
            Kind::Pistol => "Pistol",
            Kind::Rounds => "Pistol rounds",
        }
    }

    /// Whether more of it go onto one item (a stack, one cell).
    pub fn stacks(self) -> bool {
        matches!(self, Kind::Coin | Kind::Rounds)
    }

    /// Whether its count is worth showing (a stack's size, a gun's rounds).
    pub fn counted(self) -> bool {
        self.stacks() || self == Kind::Pistol
    }

    /// Cells across and down, unturned.
    pub fn size(self) -> IVec2 {
        match self {
            Kind::Crowbar => ivec2(4, 1),
            Kind::Flashlight | Kind::Pistol => ivec2(2, 1),
            Kind::Lockpicks | Kind::Coin | Kind::Rounds => ivec2(1, 1),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum At {
    Hands,
    /// The top-left cell it covers.
    Grid(IVec2),
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub kind: Kind,
    pub turned: bool,
    pub at: At,
    /// How many: a stack of coins is one item.
    pub count: u32,
}

pub fn size_of(kind: Kind, turned: bool) -> IVec2 {
    let s = kind.size();
    if turned { ivec2(s.y, s.x) } else { s }
}

impl Item {
    pub fn size(&self) -> IVec2 {
        size_of(self.kind, self.turned)
    }
}

/// The inventory's cells, across and down.
pub const GRID: IVec2 = ivec2(8, 5);

#[derive(Component)]
pub struct Inventory {
    pub items: Vec<Item>,
    /// Item indices bound to keys 1–4.
    pub quick: [Option<usize>; 4],
    pub open: bool,
}

impl Default for Inventory {
    fn default() -> Self {
        let item = |kind, at| Item {
            kind,
            turned: false,
            at,
            count: 1,
        };
        Inventory {
            items: vec![
                item(Kind::Flashlight, At::Hands),
                item(Kind::Crowbar, At::Grid(ivec2(0, 0))),
                item(Kind::Lockpicks, At::Grid(ivec2(0, 1))),
                Item {
                    count: 3,
                    ..item(Kind::Coin, At::Grid(ivec2(1, 1)))
                },
                Item {
                    count: 8,
                    ..item(Kind::Pistol, At::Grid(ivec2(2, 1)))
                },
                Item {
                    count: 16,
                    ..item(Kind::Rounds, At::Grid(ivec2(4, 1)))
                },
            ],
            quick: [Some(1), Some(0), Some(4), Some(3)],
            open: false,
        }
    }
}

impl Inventory {
    pub fn held(&self) -> Option<usize> {
        self.items.iter().position(|i| i.at == At::Hands)
    }

    pub fn holds(&self, kind: Kind) -> bool {
        self.held().is_some_and(|i| self.items[i].kind == kind)
    }

    /// Whether item `id`, turned or not, fits at `cell`, clear of everything
    /// but itself.
    pub fn fits(&self, id: usize, turned: bool, cell: IVec2) -> bool {
        self.room(self.items[id].kind, turned, cell, Some(id))
    }

    /// Whether a `kind` fits at `cell`, clear of everything but `except`.
    pub fn room(&self, kind: Kind, turned: bool, cell: IVec2, except: Option<usize>) -> bool {
        let id = except.unwrap_or(usize::MAX);
        let size = size_of(kind, turned);
        if cell.min_element() < 0 || (cell + size).cmpgt(GRID).any() {
            return false;
        }
        self.items.iter().enumerate().all(|(j, o)| match o.at {
            At::Grid(oat) if j != id => {
                (cell + size).cmple(oat).any() || (oat + o.size()).cmple(cell).any()
            }
            _ => true,
        })
    }

    /// Item `id` to `at`. Into the hands, what was there is stowed (where
    /// `id` came from, if it fits); `false`, and nothing moves, if it can't be.
    pub fn put(&mut self, id: usize, turned: bool, at: At) -> bool {
        match at {
            At::Grid(cell) if !self.fits(id, turned, cell) => return false,
            At::Grid(_) => {}
            At::Hands => {
                let from = self.items[id].at;
                let held = self.held().filter(|&h| h != id);
                self.items[id].at = At::Hands;
                if let Some(h) = held
                    && !self.stow(h)
                {
                    self.items[id].at = from;
                    return false;
                }
            }
        }
        self.items[id].turned = turned;
        self.items[id].at = at;
        true
    }

    /// Into the first place it fits, turned if need be.
    pub fn stow(&mut self, id: usize) -> bool {
        for y in 0..GRID.y {
            for x in 0..GRID.x {
                for turned in [false, true] {
                    if self.fits(id, turned, ivec2(x, y)) {
                        self.items[id].turned = turned;
                        self.items[id].at = At::Grid(ivec2(x, y));
                        return true;
                    }
                }
            }
        }
        false
    }

    /// One of item `id` used up, the item gone with the last.
    pub fn use_one(&mut self, id: usize) {
        self.items[id].count -= 1;
        if self.items[id].count == 0 {
            self.remove(id);
        }
    }

    /// Item `id` out of his things, the quick slots that pointed past it
    /// moved down.
    pub fn remove(&mut self, id: usize) -> Item {
        for q in &mut self.quick {
            *q = match *q {
                Some(i) if i == id => None,
                Some(i) if i > id => Some(i - 1),
                q => q,
            };
        }
        self.items.remove(id)
    }

    /// `n` of `kind` into his things: onto his stack of it, else at `at` if it
    /// goes there, else wherever it fits. `false`, and nothing added, if not.
    pub fn add(&mut self, kind: Kind, n: u32, at: Option<At>) -> bool {
        if kind.stacks()
            && let Some(i) = self.items.iter().position(|i| i.kind == kind)
        {
            self.items[i].count += n;
            return true;
        }
        // Off the grid until placed.
        self.items.push(Item {
            kind,
            turned: false,
            at: At::Grid(IVec2::splat(-9)),
            count: n,
        });
        let id = self.items.len() - 1;
        if at.is_some_and(|at| self.put(id, false, at)) || self.stow(id) {
            return true;
        }
        self.items.pop();
        false
    }

    /// A quick slot: its item into the hands, or out of them again.
    pub fn quick(&mut self, slot: usize) {
        let Some(id) = self.quick[slot] else {
            return;
        };
        if self.items[id].at == At::Hands {
            self.stow(id);
        } else {
            self.put(id, self.items[id].turned, At::Hands);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_keep_to_their_cells_and_quick_slots_swap_the_hands() {
        let mut inv = Inventory::default();
        // The crowbar (4×1) lies across the top row, the lockpicks under it.
        assert!(!inv.fits(2, false, ivec2(2, 0)));
        assert!(inv.fits(2, false, ivec2(6, 1)));
        assert!(!inv.fits(1, false, ivec2(5, 2)), "past the right edge");
        assert!(!inv.fits(1, true, ivec2(0, 2)), "standing, it sticks out");
        // 1 takes the crowbar; the flashlight goes back into the grid.
        inv.quick(0);
        assert!(inv.holds(Kind::Crowbar));
        assert!(matches!(inv.items[0].at, At::Grid(_)));
        // 2 the flashlight, and 2 again puts it away.
        inv.quick(1);
        assert!(inv.holds(Kind::Flashlight));
        inv.quick(1);
        assert_eq!(inv.held(), None);
        // The coins: thrown one by one, then gone, and slot 4 with them.
        inv.quick(3);
        for _ in 0..3 {
            assert!(inv.holds(Kind::Coin));
            inv.use_one(inv.held().unwrap());
        }
        assert_eq!((inv.held(), inv.quick[3]), (None, None));
        assert_eq!(inv.quick[0].map(|i| inv.items[i].kind), Some(Kind::Crowbar));
        // Taken from a guard: coins onto the stack, a new thing where it goes.
        let mut inv = Inventory::default();
        assert!(inv.add(Kind::Coin, 2, None));
        assert_eq!(inv.items[3].count, 5);
        assert!(inv.add(Kind::Lockpicks, 1, Some(At::Grid(ivec2(7, 4)))));
        assert_eq!(inv.items.last().unwrap().at, At::Grid(ivec2(7, 4)));
    }
}
