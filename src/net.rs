//! Playing together. `--server` hosts on UDP `PORT`; `--connect host[:port]`
//! joins. The server runs the bank (the guard, the doors, who is where) and
//! replicates it; each player's robber moves on his own machine and is sent
//! up every step, so his own feet never wait for the line.

use crate::doors::Leaf;
use crate::guard::Guard;
use crate::inventory::Inventory;
use crate::map::tile_centre;
use crate::movement::{Body, TILE};
use crate::player::{Controls, LocalPlayer, Player, spawn_robber};
use crate::smash::Bank;
use crate::sound::Noise;
use bevy::ecs::component::Mutable;
use bevy::prelude::*;
use bevy_replicon::bytes::Bytes;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::registry::ctx::WriteCtx;
use bevy_replicon::shared::replication::registry::receive_fns::default_remove;
use bevy_replicon_renet::netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};
use bevy_replicon_renet::renet::ConnectionConfig;
use bevy_replicon_renet::{RenetChannelsExt, RenetClient, RenetServer, RepliconRenetPlugins};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::SystemTime;

pub const PORT: u16 = 8794;
/// Bump when what crosses the line changes: older builds are refused.
const PROTOCOL: u64 = 1;
const MAX_PLAYERS: usize = 6;

/// The bank's seed, on a replicated entity: a client builds the same bank.
#[derive(Component, Serialize, Deserialize)]
pub struct Seed(pub u64);

/// The client a robber is played from.
#[derive(Component, Serialize, Deserialize)]
pub struct Owner(pub u64);

/// This machine's client id; present only while playing on a server.
#[derive(Resource)]
pub struct Joined(u64);

/// A robber's state after his step, sent up by his player.
#[derive(Message, Serialize, Deserialize)]
struct Moved {
    body: Body,
    torch: bool,
}

/// His things, sent up when they change.
#[derive(Message, Serialize, Deserialize)]
struct Packed(Inventory);

pub enum Role {
    Alone,
    Server(u16),
    Client(SocketAddr),
}

/// From the command line: `--server [port]` or `--connect host[:port]`.
pub fn role() -> Role {
    let args: Vec<String> = std::env::args().collect();
    let after = |flag: &str| {
        let i = args.iter().position(|a| a == flag)?;
        Some(args.get(i + 1).cloned().unwrap_or_default())
    };
    if let Some(port) = after("--server") {
        return Role::Server(port.parse().unwrap_or(PORT));
    }
    let Some(host) = after("--connect") else {
        return Role::Alone;
    };
    let host = if host.contains(':') {
        host
    } else {
        format!("{host}:{PORT}")
    };
    // The client's socket is IPv4.
    let addr = host
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.find(SocketAddr::is_ipv4))
        .unwrap_or_else(|| panic!("cannot find {host}"));
    Role::Client(addr)
}

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RepliconPlugins, RepliconRenetPlugins))
            .replicate::<Seed>()
            .replicate::<Owner>()
            .replicate::<Player>()
            .replicate::<Controls>()
            .replicate::<Inventory>()
            .replicate::<Guard>()
            .replicate::<Leaf>()
            .add_client_message::<Moved>(Channel::Unreliable)
            .add_client_message::<Packed>(Channel::Ordered)
            .add_server_message::<Noise>(Channel::Unordered)
            .register_marker::<LocalPlayer>()
            .set_marker_fns::<LocalPlayer, Player>(ours::<Player>, default_remove::<Player>)
            .set_marker_fns::<LocalPlayer, Controls>(ours::<Controls>, default_remove::<Controls>)
            .set_marker_fns::<LocalPlayer, Inventory>(
                ours::<Inventory>,
                default_remove::<Inventory>,
            )
            .add_systems(Startup, open)
            .add_observer(join)
            .add_observer(leave)
            .add_observer(mine)
            .add_systems(
                PreUpdate,
                (moved, packed)
                    .after(ServerSystems::Receive)
                    .run_if(in_state(ServerState::Running)),
            )
            .add_systems(
                PostUpdate,
                (
                    pass_on
                        .before(ServerSystems::Send)
                        .run_if(in_state(ServerState::Running)),
                    send.before(ClientSystems::Send)
                        .run_if(in_state(ClientState::Connected)),
                ),
            );
    }
}

/// The server's word on his own robber is taken only to make him: after
/// that, his player's machine knows better.
fn ours<C: Component<Mutability = Mutable>>(
    ctx: &mut WriteCtx,
    rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity,
    message: &mut Bytes,
) -> Result<()> {
    let c: C = rule_fns.deserialize(ctx, message)?;
    if !entity.contains::<C>() {
        entity.insert(c);
    }
    Ok(())
}

fn open(mut commands: Commands, channels: Res<RepliconChannels>) -> Result<()> {
    let config = || ConnectionConfig {
        server_channels_config: channels.server_configs(),
        client_channels_config: channels.client_configs(),
        ..default()
    };
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    match role() {
        Role::Alone => {}
        Role::Server(port) => {
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
            let transport = NetcodeServerTransport::new(
                ServerConfig {
                    current_time: now,
                    max_clients: MAX_PLAYERS,
                    protocol_id: PROTOCOL,
                    authentication: ServerAuthentication::Unsecure,
                    public_addresses: default(),
                },
                socket,
            )?;
            info!("serving on UDP port {port}");
            commands.insert_resource(RenetServer::new(config()));
            commands.insert_resource(transport);
        }
        Role::Client(server_addr) => {
            let client_id = now.as_nanos() as u64 ^ std::process::id() as u64;
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
            let authentication = ClientAuthentication::Unsecure {
                client_id,
                protocol_id: PROTOCOL,
                server_addr,
                user_data: None,
            };
            info!("connecting to {server_addr}");
            commands.insert_resource(RenetClient::new(config()));
            commands.insert_resource(NetcodeClientTransport::new(now, authentication, socket)?);
            commands.insert_resource(Joined(client_id));
        }
    }
    Ok(())
}

/// A player connects: his robber walks in at the bank's spawn.
fn join(
    add: On<Add, NetworkId>,
    ids: Query<&NetworkId>,
    bank: Option<Res<Bank>>,
    robbers: Query<&Player>,
    mut commands: Commands,
) {
    let (Ok(id), Some(bank)) = (ids.get(add.entity), bank) else {
        return;
    };
    info!("player {} joined", id.get());
    // The free tile nearest the spawn with nobody on it.
    let map = &bank.0;
    let free = map.free();
    let near = |p: &Vec2| p.distance(map.spawn);
    let at = (0..map.h)
        .flat_map(|y| (0..map.w).map(move |x| (x, y)))
        .filter(|&(x, y)| free[map.idx(x, y)])
        .map(|(x, y)| tile_centre(x, y))
        .filter(|p| robbers.iter().all(|r| r.body.pos.distance(*p) > TILE))
        .min_by(|a, b| near(a).total_cmp(&near(b)))
        .unwrap_or(map.spawn);
    let robber = spawn_robber(&mut commands, at, false);
    commands.entity(robber).insert(Owner(id.get()));
}

fn leave(
    remove: On<Remove, NetworkId>,
    ids: Query<&NetworkId>,
    robbers: Query<(Entity, &Owner)>,
    mut commands: Commands,
) {
    let Ok(id) = ids.get(remove.entity) else {
        return;
    };
    info!("player {} left", id.get());
    for (e, owner) in &robbers {
        if owner.0 == id.get() {
            commands.entity(e).despawn();
        }
    }
}

/// On a client, the robber the server made for this machine is the one played.
fn mine(
    add: On<Add, Owner>,
    owners: Query<&Owner>,
    joined: Option<Res<Joined>>,
    mut commands: Commands,
) {
    if let (Ok(owner), Some(joined)) = (owners.get(add.entity), joined)
        && owner.0 == joined.0
    {
        commands.entity(add.entity).insert(LocalPlayer);
    }
}

fn id_of(client: ClientId, ids: &Query<&NetworkId>) -> Option<u64> {
    Some(ids.get(client.entity()?).ok()?.get())
}

fn moved(
    mut moves: MessageReader<FromClient<Moved>>,
    ids: Query<&NetworkId>,
    mut robbers: Query<(&Owner, &mut Player, &mut Controls)>,
) {
    for FromClient { client_id, message } in moves.read() {
        let id = id_of(*client_id, &ids);
        for (_, mut player, mut controls) in robbers.iter_mut().filter(|r| Some(r.0.0) == id) {
            player.body = message.body.clone();
            controls.torch = message.torch;
        }
    }
}

fn packed(
    mut packs: MessageReader<FromClient<Packed>>,
    ids: Query<&NetworkId>,
    mut robbers: Query<(&Owner, &mut Inventory)>,
) {
    for FromClient { client_id, message } in packs.read() {
        let id = id_of(*client_id, &ids);
        for (_, mut inventory) in robbers.iter_mut().filter(|r| Some(r.0.0) == id) {
            *inventory = message.0.clone();
        }
    }
}

/// Noises made on the server, for the players to hear.
fn pass_on(mut noises: MessageReader<Noise>, mut out: MessageWriter<ToClients<Noise>>) {
    for noise in noises.read() {
        out.write(ToClients {
            targets: SendTargets::CLIENTS_ONLY,
            message: noise.clone(),
        });
    }
}

type Mine<'a> = (Ref<'a, Player>, &'a Controls, Ref<'a, Inventory>);

fn send(
    me: Single<Mine, With<LocalPlayer>>,
    mut moves: MessageWriter<Moved>,
    mut packs: MessageWriter<Packed>,
) {
    let (player, controls, inventory) = *me;
    if player.is_changed() {
        moves.write(Moved {
            body: player.body.clone(),
            torch: controls.torch,
        });
    }
    if inventory.is_changed() {
        packs.write(Packed(Inventory::clone(&inventory)));
    }
}
