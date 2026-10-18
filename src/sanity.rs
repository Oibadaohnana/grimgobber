//! Sanity, per robber, 0 to 100: it drains in the bank's dark rooms (slowly
//! in lit ones, not at all outside) and when the wraith shows itself, and the
//! lower it is, the likelier a roll of the wraith's comes up a haunt. The
//! server keeps it.

use crate::map::tile_of;
use crate::player::{Dead, Player};
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
    }
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
