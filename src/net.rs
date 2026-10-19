//! Playing together. `--server` hosts on UDP `PORT`; `--connect host[:port]`
//! joins. The server runs the bank (the wraith, the doors, who is where) and
//! replicates it; each player's hunter moves on his own machine and is sent
//! up every step, so his own feet never wait for the line.

use crate::capture::{Seal, Shot};
use crate::crew::{Crewmate, Leave, Look, Ready, SetLook};
use crate::door::{Answer, Asking, FIRST, IDLE};
use crate::doors::Leaf;
use crate::end::Ended;
use crate::instruments::Emf;
use crate::inventory::Inventory;
use crate::junk::Loose;
use crate::movement::Body;
use crate::player::{Controls, Dead, LocalPlayer, Player};
use crate::power::{Flip, Power, Reset};
use crate::sanity::Sanity;
use crate::sound::Noise;
use crate::talk::Say;
use crate::throw::Toss;
use crate::trap::{Pull, SetTrap, TakeTrap, Trap};
use crate::wraith::{Note, Omen, Prints, Wraith};
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
pub const PROTOCOL: u64 = 13;
/// The game's version (Cargo.toml's), and the protocol it speaks.
pub const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));
const MAX_PLAYERS: usize = 6;

/// The bank's seed, on a replicated entity: a client builds the same bank.
#[derive(Component, Serialize, Deserialize)]
pub struct Seed(pub u64);

/// The client a hunter is played from.
#[derive(Component, Serialize, Deserialize)]
pub struct Owner(pub u64);

/// This machine's client id; present only while playing on a server.
#[derive(Resource)]
pub struct Joined(u64);

/// A hunter's state after his step, sent up by his player.
#[derive(Message, Serialize, Deserialize)]
struct Moved {
    body: Body,
    torch: bool,
    uv: bool,
}

/// His things, sent up when they change.
#[derive(Message, Serialize, Deserialize)]
struct Packed(Inventory);

pub enum Role {
    Alone,
    Server(u16),
    Client(SocketAddr),
}

/// What this machine is doing: at the title, playing alone, or in a lobby on
/// a server (its code, if it came by the door).
#[derive(Resource, Clone, PartialEq)]
pub enum Mode {
    Title,
    Alone,
    Online(Option<String>),
    /// `--sim`: a bank alone to try the wraith in (sim.rs).
    Sim,
}

/// The title screen's line: what it waits on, or why it is back.
#[derive(Resource, Default)]
pub struct TitleLine(pub String);

/// A question to the door under way.
#[derive(Resource)]
pub struct Knocking(pub Asking);

/// The value after `flag` on the command line, if the flag is there.
fn arg(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == flag)?;
    Some(args.get(i + 1).cloned().unwrap_or_default())
}

/// `--sim`: a bank alone to try the wraith in.
pub fn sim() -> bool {
    std::env::args().any(|a| a == "--sim")
}

/// `--serve [port]`: run the front door (door.rs) instead of a game.
pub fn serve() -> Option<u16> {
    arg("--serve").map(|p| p.parse().unwrap_or(PORT))
}

/// `--code CODE`: the lobby a game server is (from the door).
pub fn code() -> Option<String> {
    arg("--code").filter(|c| !c.is_empty())
}

/// The door to knock at: `--door host[:port]`, else grim.buggly.de.
pub fn door() -> String {
    let host = arg("--door").unwrap_or_else(|| crate::door::HOST.into());
    if host.contains(':') {
        host
    } else {
        format!("{host}:{PORT}")
    }
}

/// From the command line: `--server [port]` or `--connect host[:port]`.
pub fn role() -> Role {
    if let Some(port) = arg("--server") {
        return Role::Server(port.parse().unwrap_or(PORT));
    }
    let Some(host) = arg("--connect") else {
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
        if matches!(role(), Role::Server(_)) && code().is_some_and(|c| c != FIRST) {
            app.add_systems(Update, idle);
        }
        app.add_plugins((RepliconPlugins, RepliconRenetPlugins))
            .replicate::<Seed>()
            .replicate::<Owner>()
            .replicate::<Player>()
            .replicate::<Controls>()
            .replicate::<Inventory>()
            .replicate::<Leaf>()
            .replicate::<Wraith>()
            .replicate::<Note>()
            .replicate::<Trap>()
            .replicate::<Loose>()
            .replicate::<Emf>()
            .replicate::<Power>()
            .replicate::<Crewmate>()
            .replicate::<Look>()
            .replicate::<Sanity>()
            .replicate::<Dead>()
            .add_client_message::<Moved>(Channel::Unreliable)
            .add_client_message::<Packed>(Channel::Ordered)
            .add_client_message::<Say>(Channel::Ordered)
            .add_client_message::<SetTrap>(Channel::Ordered)
            .add_client_message::<TakeTrap>(Channel::Ordered)
            .add_client_message::<Pull>(Channel::Ordered)
            .add_client_message::<Shot>(Channel::Ordered)
            .add_client_message::<Seal>(Channel::Ordered)
            .add_client_message::<Toss>(Channel::Ordered)
            .add_client_message::<Flip>(Channel::Ordered)
            .add_client_message::<Reset>(Channel::Ordered)
            .add_client_message::<SetLook>(Channel::Ordered)
            .add_client_message::<Ready>(Channel::Ordered)
            .add_client_message::<Leave>(Channel::Ordered)
            .add_server_message::<Noise>(Channel::Unordered)
            .add_server_message::<Prints>(Channel::Unordered)
            .add_server_message::<Omen>(Channel::Unordered)
            .add_server_message::<Ended>(Channel::Ordered)
            .register_marker::<LocalPlayer>()
            .set_marker_fns::<LocalPlayer, Player>(ours::<Player>, default_remove::<Player>)
            .set_marker_fns::<LocalPlayer, Controls>(ours::<Controls>, default_remove::<Controls>)
            .set_marker_fns::<LocalPlayer, Inventory>(
                ours::<Inventory>,
                default_remove::<Inventory>,
            )
            .init_resource::<TitleLine>()
            .add_systems(Startup, open)
            .add_systems(Update, (knock, lost))
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
                    (
                        pass_on::<Noise>,
                        pass_on::<Prints>,
                        pass_on::<Omen>,
                        pass_on::<Ended>,
                    )
                        .before(ServerSystems::Send)
                        .run_if(in_state(ServerState::Running)),
                    send.before(ClientSystems::Send)
                        .run_if(in_state(ClientState::Connected)),
                ),
            );
    }
}

/// The server's word on his own hunter is taken only to make him: after
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

fn config(channels: &RepliconChannels) -> ConnectionConfig {
    ConnectionConfig {
        server_channels_config: channels.server_configs(),
        client_channels_config: channels.client_configs(),
        ..default()
    }
}

/// Connects to the game server at `server_addr`, now.
pub fn connect(
    commands: &mut Commands,
    channels: &RepliconChannels,
    server_addr: SocketAddr,
) -> Result<()> {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    let client_id = now.as_nanos() as u64 ^ std::process::id() as u64;
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL,
        server_addr,
        user_data: None,
    };
    info!("connecting to {server_addr}");
    commands.insert_resource(RenetClient::new(config(channels)));
    commands.insert_resource(NetcodeClientTransport::new(now, authentication, socket)?);
    commands.insert_resource(Joined(client_id));
    Ok(())
}

fn open(mut commands: Commands, channels: Res<RepliconChannels>) -> Result<()> {
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
            let lobby = code().unwrap_or_default();
            info!("serving lobby {lobby} on UDP port {port}");
            commands.insert_resource(RenetServer::new(config(&channels)));
            commands.insert_resource(transport);
        }
        Role::Client(server_addr) => connect(&mut commands, &channels, server_addr)?,
    }
    Ok(())
}

/// The door's answer, once it comes: into that lobby, or why not.
fn knock(
    time: Res<Time>,
    knocking: Option<ResMut<Knocking>>,
    channels: Res<RepliconChannels>,
    mut mode: ResMut<Mode>,
    mut line: ResMut<TitleLine>,
    mut commands: Commands,
) {
    let Some(mut knocking) = knocking else {
        return;
    };
    let Some(answer) = knocking.0.poll(time.delta_secs()) else {
        return;
    };
    commands.remove_resource::<Knocking>();
    line.0 = match answer {
        Ok(Answer::Lobby(code, addr)) => match connect(&mut commands, &channels, addr) {
            Ok(()) => {
                *mode = Mode::Online(Some(code));
                String::new()
            }
            Err(e) => format!("cannot connect: {e}"),
        },
        Ok(Answer::None) => "No lobby has that code.".into(),
        Ok(Answer::Full) => "The server has no room for another lobby.".into(),
        Ok(Answer::Old) => "Your game is not the server's version: update it.".into(),
        Err(e) => e,
    };
}

/// Off the server (left, refused or lost): what came from it goes, and it
/// is the title again.
#[allow(clippy::too_many_arguments)]
fn lost(
    state: Res<State<ClientState>>,
    remote: Query<Entity, With<Remote>>,
    mut mode: ResMut<Mode>,
    mut line: ResMut<TitleLine>,
    mut tried: Local<bool>,
    mut commands: Commands,
) {
    if *state.get() != ClientState::Disconnected {
        *tried = true;
        return;
    }
    if !std::mem::take(&mut *tried) {
        return;
    }
    for e in &remote {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<RenetClient>();
    commands.remove_resource::<NetcodeClientTransport>();
    commands.remove_resource::<Joined>();
    if matches!(*mode, Mode::Online(_)) {
        line.0 = "The connection to the lobby is gone.".into();
    }
    *mode = Mode::Title;
}

/// A lobby a player opened closes once nobody has been in it for `IDLE`.
fn idle(
    time: Res<Time>,
    crew: Query<(), With<Crewmate>>,
    mut empty: Local<f32>,
    mut exit: MessageWriter<AppExit>,
) {
    if !crew.is_empty() {
        *empty = 0.0;
        return;
    }
    *empty += time.delta_secs();
    let idle = crate::dev::env("BB_IDLE")
        .and_then(|s| s.parse().ok())
        .unwrap_or(IDLE);
    if *empty >= idle {
        info!("the lobby has been empty for {idle} s: closing");
        exit.write(AppExit::Success);
    }
}

/// A player connects: a crewmate in the lobby.
fn join(add: On<Add, NetworkId>, ids: Query<&NetworkId>, mut commands: Commands) {
    let Ok(id) = ids.get(add.entity) else {
        return;
    };
    info!("player {} joined", id.get());
    commands.spawn((
        Replicated,
        Crewmate {
            id: id.get(),
            ready: false,
            in_round: false,
        },
    ));
}

fn leave(
    remove: On<Remove, NetworkId>,
    ids: Query<&NetworkId>,
    hunters: Query<(Entity, &Owner)>,
    crew: Query<(Entity, &Crewmate)>,
    mut commands: Commands,
) {
    let Ok(id) = ids.get(remove.entity) else {
        return;
    };
    info!("player {} left", id.get());
    let mine = hunters.iter().filter(|r| r.1.0 == id.get()).map(|r| r.0);
    for e in mine.chain(crew.iter().filter(|c| c.1.id == id.get()).map(|c| c.0)) {
        commands.entity(e).despawn();
    }
}

/// The hunter made for this machine's crewmate is the one played here.
fn mine(
    add: On<Add, Owner>,
    owners: Query<&Owner>,
    joined: Option<Res<Joined>>,
    headless: Option<Res<crate::Headless>>,
    mut commands: Commands,
) {
    if let Ok(owner) = owners.get(add.entity)
        && headless.is_none()
        && owner.0 == my_id(joined.as_deref())
    {
        commands.entity(add.entity).insert(LocalPlayer);
    }
}

/// This machine's crewmate's id: its client id, or 0 alone.
pub fn my_id(joined: Option<&Joined>) -> u64 {
    joined.map_or(0, |j| j.0)
}

/// Whose a message is: the client's id, or 0 from the one playing alone.
pub fn client_u64(client: ClientId, ids: &Query<&NetworkId>) -> u64 {
    id_of(client, ids).unwrap_or(0)
}

fn id_of(client: ClientId, ids: &Query<&NetworkId>) -> Option<u64> {
    Some(ids.get(client.entity()?).ok()?.get())
}

/// Whether a hunter is the one a message came from: alone, the one played here.
pub fn sent_by(
    client: ClientId,
    ids: &Query<&NetworkId>,
    owner: Option<&Owner>,
    local: bool,
) -> bool {
    match client.entity() {
        None => local,
        Some(_) => owner.is_some_and(|o| Some(o.0) == id_of(client, ids)),
    }
}

fn moved(
    mut moves: MessageReader<FromClient<Moved>>,
    ids: Query<&NetworkId>,
    mut hunters: Query<(&Owner, &mut Player, &mut Controls)>,
) {
    for FromClient { client_id, message } in moves.read() {
        let id = id_of(*client_id, &ids);
        for (_, mut player, mut controls) in hunters.iter_mut().filter(|r| Some(r.0.0) == id) {
            player.body = message.body.clone();
            controls.torch = message.torch;
            controls.uv = message.uv;
        }
    }
}

fn packed(
    mut packs: MessageReader<FromClient<Packed>>,
    ids: Query<&NetworkId>,
    mut hunters: Query<(&Owner, &mut Inventory)>,
) {
    for FromClient { client_id, message } in packs.read() {
        let id = id_of(*client_id, &ids);
        for (_, mut inventory) in hunters.iter_mut().filter(|r| Some(r.0.0) == id) {
            *inventory = message.0.clone();
        }
    }
}

/// Noises and footprints made on the server, for the players.
fn pass_on<M: Message + Clone>(mut made: MessageReader<M>, mut out: MessageWriter<ToClients<M>>) {
    for m in made.read() {
        out.write(ToClients {
            targets: SendTargets::CLIENTS_ONLY,
            message: m.clone(),
        });
    }
}

type Mine<'a> = (Ref<'a, Player>, Ref<'a, Controls>, Ref<'a, Inventory>);

fn send(
    me: Single<Mine, With<LocalPlayer>>,
    mut moves: MessageWriter<Moved>,
    mut packs: MessageWriter<Packed>,
) {
    let (player, controls, inventory) = *me;
    // Every change of his lights too, standing still or not.
    if player.is_changed() || controls.is_changed() {
        moves.write(Moved {
            body: player.body.clone(),
            torch: controls.torch,
            uv: controls.uv,
        });
    }
    if inventory.is_changed() {
        packs.write(Packed(Inventory::clone(&inventory)));
    }
}
