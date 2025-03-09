use bevy::{prelude::*, window::PrimaryWindow};
use crossbeam_channel::*;
use serde::{Deserialize, Serialize};
use steamworks::{
    networking_types::{NetConnectionEnd, NetworkingIdentity, NetworkingMessage, SendFlags},
    *,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(SteamClient::new())
        .add_systems(Startup, setup)
        .add_systems(Update, update)
        .run();
}

#[derive(Resource)]
struct SteamClient {
    client: Client,
    lobby_id: Option<LobbyId>,
    channel: SteamChannel,
}

#[derive(Serialize, Deserialize, Debug)]
struct NetworkData {
    data: Vec<u8>,
}

impl SteamClient {
    pub fn new() -> SteamClient {
        SteamClient {
            client: Client::init_app(480).unwrap(),
            lobby_id: None,
            channel: SteamChannel::new(),
        }
    }

    pub fn steam_id(&self) -> SteamId {
        self.client.user().steam_id()
    }

    pub fn is_in_lobby(&self) -> bool {
        self.lobby_id.is_some()
    }

    pub fn join_lobby(&self, lobby_id: LobbyId) {
        let tx = self.channel.sender.clone();
        self.client.matchmaking().join_lobby(lobby_id, move |res| {
            if let Ok(lobby_id) = res {
                match tx.send(ChannelMessage::LobbyJoined(lobby_id)) {
                    Ok(_) => {}
                    Err(_) => {}
                }
            }
        });
    }

    pub fn get_players_in_lobby(&self) -> Vec<SteamId> {
        let Some(lobby_id) = self.lobby_id else {
            panic!("Not currently in a lobby");
        };
        self.client.matchmaking().lobby_members(lobby_id)
    }

    pub fn send_message_others(&self, data: NetworkData, flags: SendFlags) -> Result<(), String> {
        if !self.is_in_lobby() {
            return Err("Not currently in a lobby".to_owned());
        };
        for player in self.get_players_in_lobby() {
            if player == self.steam_id() {
                continue;
            }
            self.send_message(&data, player, flags)
                .expect("Couldn't send message in send others");
        }
        return Ok(());
    }

    pub fn send_message(
        &self,
        data: &NetworkData,
        target: SteamId,
        flags: SendFlags,
    ) -> Result<(), String> {
        if !self.is_in_lobby() {
            return Err("Not in a lobby".to_string());
        };
        let serialize_data = rmp_serde::to_vec(&data);
        let serialized = serialize_data.map_err(|err| err.to_string())?;
        let data_arr = serialized.as_slice();
        let network_identity = NetworkingIdentity::new_steam_id(target);
        let res = self.client.networking_messages().send_message_to_user(
            network_identity,
            flags,
            data_arr,
            0,
        );
        println!("Sent message: {:?} {:?}", data, res);
        return res.map_err(|e: SteamError| e.to_string());
    }
}

enum ChannelMessage {
    LobbyCreated(LobbyId),
    LobbyJoinRequest(LobbyId),
    LobbyJoined(LobbyId),
    LobbyChatMessage(LobbyChatMsg),
    LobbyChatUpdate(LobbyChatUpdate),
}

struct SteamChannel {
    sender: Sender<ChannelMessage>,
    receiver: Receiver<ChannelMessage>,
}

impl SteamChannel {
    pub fn new() -> SteamChannel {
        let (sender, receiver) = unbounded();
        SteamChannel { sender, receiver }
    }
}

fn setup(mut commands: Commands, client: Res<SteamClient>) {
    client
        .client
        .networking_messages()
        .session_request_callback(move |req| {
            println!("Accepting session request from {:?}", req.remote());
            req.accept();
        });

    // Install a callback to debug print failed peer connections
    client
        .client
        .networking_messages()
        .session_failed_callback(|info| {
            eprintln!("Session failed: {info:#?}");
        });

    commands.spawn(Camera2d);
}

fn update(
    mut gizmos: Gizmos,
    steam_client: Res<SteamClient>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform)>,
) {
    steam_client.client.run_callbacks();

    // Draw us at our mouse position
    // get the camera info and transform
    // assuming there is exactly one main camera entity, so Query::single() is OK
    let (camera, camera_transform) = q_camera.single();

    // There is only one primary window, so we can similarly get it from the query:
    let window = q_window.single();

    // check if the cursor is inside the window and get its position
    // then, ask bevy to convert into world coordinates, and truncate to discard Z
    if let Some(position) = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor).ok())
        .map(|ray| ray.origin.truncate())
    {
        println!("position: {}", position);
        gizmos.circle_2d(Isometry2d::from_translation(position), 10., Color::WHITE);

        // Send our mouse position to all friends
        for friend in steam_client
            .client
            .friends()
            .get_friends(FriendFlags::IMMEDIATE)
        {
            let identity = NetworkingIdentity::new_steam_id(friend.id());

            // Convert our position to bytes
            let mut data = [0; 8];
            data[0..4].copy_from_slice(&position.x.to_le_bytes());
            data[4..8].copy_from_slice(&position.y.to_le_bytes());

            let _ = steam_client
                .client
                .networking_messages()
                .send_message_to_user(identity, SendFlags::UNRELIABLE_NO_DELAY, &data, 0);
        }
    }

    let mut peers: Vec<(String, Vec2)> = Vec::new();

    // Receive messages from the network
    for message in steam_client
        .client
        .networking_messages()
        .receive_messages_on_channel(0, 100)
    {
        let peer = message.identity_peer();
        let data = message.data();

        // Convert peer position from bytes
        let peer_x = f32::from_le_bytes(data[0..4].try_into().expect("Someone sent bad message"));
        let peer_y = f32::from_le_bytes(data[4..8].try_into().expect("Someone sent bad message"));

        peers.push((peer.debug_string(), Vec2::new(peer_x, peer_y)));
    }

    // Draw all peers
    for peer in peers {
        gizmos.circle_2d(Isometry2d::from_translation(peer.1), 10., Color::WHITE);
    }
}
