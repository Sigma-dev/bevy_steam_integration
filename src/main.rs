use bevy::{prelude::*, utils::hashbrown::HashMap};
use crossbeam_channel::*;
use serde::{Deserialize, Serialize};
use steamworks::{
    networking_sockets::NetConnection,
    networking_types::{
        NetConnectionEnd, NetConnectionStatusChanged, NetworkingConfigEntry, NetworkingIdentity,
        NetworkingMessage, SendFlags,
    },
    *,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(SteamClient::new())
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_receivers, update))
        .add_systems(PreUpdate, receive_messages)
        .run();
}

#[derive(Resource)]
struct SteamClient {
    client: Client,
    lobby_id: Option<LobbyId>,
    channel: SteamChannel,
    sockets: HashMap<SteamId, NetConnection<ClientManager>>,
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
            sockets: HashMap::new(),
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
        /*  let res = self.client.networking_messages().send_message_to_user(
            network_identity,
            flags,
            data_arr,
            0,
        ); */
        let res = self
            .client
            .networking()
            .send_p2p_packet(target, SendType::Reliable, data_arr);
        println!("Sent message: {:?} {:?}", data, res);
        return Ok(());
        // return res.map_err(|e: SteamError| e.to_string());
    }
}

enum ChannelMessage {
    LobbyCreated(LobbyId),
    LobbyJoinRequest(LobbyId),
    LobbyJoined(LobbyId),
    LobbyChatMessage(LobbyChatMsg),
    LobbyChatUpdate(LobbyChatUpdate),
    SessionRequest(P2PSessionRequest),
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

fn setup(client: Res<SteamClient>) {
    let sender = client.channel.sender.clone();
    let tx = client.channel.sender.clone();
    let tx2 = client.channel.sender.clone();
    let tx3 = tx2.clone();

    client.client.networking_utils().init_relay_network_access();

    client
        .client
        .register_callback(move |message: LobbyChatMsg| {
            println!("Lobby chat message received: {:?}", message);
            sender
                .send(ChannelMessage::LobbyChatMessage(message))
                .unwrap();
        });
    let _request_callback = client
        .client
        .register_callback(move |request: P2PSessionRequest| {
            println!("ACCEPTED PEER");
            let _ = tx3.send(ChannelMessage::SessionRequest(request));
        });

    client
        .client
        .register_callback(move |message: LobbyChatUpdate| {
            println!("Lobby update  received: {:?}", message);
            tx2.clone()
                .send(ChannelMessage::LobbyChatUpdate(message))
                .unwrap();
        });

    client
        .client
        .register_callback(move |message: GameLobbyJoinRequested| {
            let _ = tx.send(ChannelMessage::LobbyJoinRequest(message.lobby_steam_id));
        });

    client
        .client
        .networking_messages()
        .session_request_callback(move |session_request| {
            println!("Received session request");
            session_request.accept();
        });

    client
        .client
        .networking_messages()
        .session_failed_callback(move |res| {
            println!(
                "Session Failed: {:?}",
                res.end_reason().unwrap_or(NetConnectionEnd::Other(-42))
            );
        });
}

fn receive_messages(client: Res<SteamClient>) {
    let messages: Vec<NetworkingMessage<ClientManager>> = client
        .client
        .networking_messages()
        .receive_messages_on_channel(0, 2048);

    for message in messages {
        println!("Received message: {:?}", message.data());
        let sender = message.identity_peer().steam_id().unwrap();
        let serialized_data = message.data();
        let data_try: Result<NetworkData, _> = rmp_serde::from_slice(serialized_data);

        if let Ok(data) = data_try {
            println!("Decoded: {:?}", data);
        }
        drop(message); //not sure about usefullness, mentionned in steam docs as release
    }

    let mut buf = [0 as u8; 4096];
    loop {
        let msg2 = client.client.networking().read_p2p_packet(&mut buf);
        let Some(msg) = msg2 else {
            break;
        };
        println!("msg: {:?}", buf)
    }
}

fn handle_receivers(mut steam_client: ResMut<SteamClient>) {
    let tx: Sender<ChannelMessage> = steam_client.channel.sender.clone();
    if let Ok(msg) = steam_client.channel.receiver.try_recv() {
        match msg {
            ChannelMessage::LobbyCreated(lobby_id) => {
                steam_client.lobby_id = Some(lobby_id);
                info!("Created lobby {:?}", lobby_id);
            }
            ChannelMessage::LobbyJoined(lobby_id) => {
                steam_client.lobby_id = Some(lobby_id);
                for player in steam_client.client.matchmaking().lobby_members(lobby_id) {
                    let connection = steam_client.client.networking_sockets().connect_p2p(
                        NetworkingIdentity::new_steam_id(player),
                        0,
                        [],
                    );
                    steam_client
                        .sockets
                        .insert(player, connection.expect("Socket connection failed :("));
                }
                info!("Joined lobby {:?}", lobby_id)
            }
            ChannelMessage::LobbyChatMessage(message) => {
                let mut buffer = vec![0; 256];
                let buffer = steam_client.client.matchmaking().get_lobby_chat_entry(
                    message.lobby,
                    message.chat_id,
                    buffer.as_mut_slice(),
                );
                info!("Message buffer: [{:?}]", buffer);
            }
            ChannelMessage::LobbyChatUpdate(update) => {
                info!("Update: {:?}", update);
            }
            ChannelMessage::LobbyJoinRequest(lobby_id) => {
                info!("Requested to join lobby {:?}", lobby_id);
                steam_client.join_lobby(lobby_id);
            }
            ChannelMessage::SessionRequest(session_request) => {
                info!("Session request: {:?}", session_request);
                steam_client
                    .client
                    .networking()
                    .accept_p2p_session(session_request.remote);
            }
        };
    }
}

fn update(keys: Res<ButtonInput<KeyCode>>, steam_client: Res<SteamClient>) {
    steam_client.client.run_callbacks();
    let matchmaking = steam_client.client.matchmaking();
    let tx = steam_client.channel.sender.clone();

    if keys.just_pressed(KeyCode::KeyC) {
        matchmaking.create_lobby(LobbyType::FriendsOnly, 4, move |result| match result {
            Ok(lobby_id) => {
                tx.send(ChannelMessage::LobbyCreated(lobby_id)).unwrap();
                println!("Created lobby: [{}]", lobby_id.raw())
            }
            Err(err) => panic!("Error: {}", err),
        });
    }

    if keys.just_pressed(KeyCode::KeyT) {
        let _ =
            steam_client.send_message_others(NetworkData { data: vec![42] }, SendFlags::RELIABLE);
    }

    if keys.just_pressed(KeyCode::KeyJ) {
        for friend in steam_client.client.friends().get_friends(FriendFlags::ALL) {
            if let Some(game) = friend.game_played() {
                if game.game.app_id() == AppId(480) {
                    steam_client.join_lobby(game.lobby);
                    println!("Auto join");
                }
            }
        }
    }
}
