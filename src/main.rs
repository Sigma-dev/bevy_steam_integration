use bevy::prelude::*;
use crossbeam_channel::*;
use steamworks::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(SteamClient::new())
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_receivers, update))
        .run();
}

#[derive(Resource)]
struct SteamClient {
    client: Client,
    lobby_id: Option<LobbyId>,
    channel: SteamChannel,
}

impl SteamClient {
    pub fn new() -> SteamClient {
        SteamClient {
            client: Client::init_app(480).unwrap(),
            lobby_id: None,
            channel: SteamChannel::new(),
        }
    }
}

enum ChannelMessage {
    LobbyCreated(LobbyId),
    LobbyJoinRequest(LobbyId),
    LobbyJoined(LobbyId),
    LobbyChat(LobbyChatMsg),
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

    // Register the callback with the cloned sender
    client
        .client
        .register_callback(move |message: LobbyChatMsg| {
            println!("Lobby chat message received: {:?}", message);
            sender.send(ChannelMessage::LobbyChat(message)).unwrap();
        });

    client
        .client
        .register_callback(move |message: GameLobbyJoinRequested| {
            tx.send(ChannelMessage::LobbyJoinRequest(message.lobby_steam_id));
        });
}

fn handle_receivers(mut steam_client: ResMut<SteamClient>) {
    let tx: Sender<ChannelMessage> = steam_client.channel.sender.clone();
    if let Ok(msg) = steam_client.channel.receiver.try_recv() {
        match msg {
            ChannelMessage::LobbyCreated(lobby_id) => {
                steam_client.lobby_id = Some(lobby_id);
                debug!("Created lobby {:?}", lobby_id);
            }
            ChannelMessage::LobbyJoined(lobby_id) => {
                steam_client.lobby_id = Some(lobby_id);
                debug!("Joined lobby {:?}", lobby_id)
            }
            ChannelMessage::LobbyChat(message) => {
                let mut buffer = vec![0; 256];
                let buffer = steam_client.client.matchmaking().get_lobby_chat_entry(
                    message.lobby,
                    message.chat_id,
                    buffer.as_mut_slice(),
                );
                println!("Message buffer: [{:?}]", buffer);
            }
            ChannelMessage::LobbyJoinRequest(lobby_id) => {
                steam_client
                    .client
                    .matchmaking()
                    .join_lobby(lobby_id, move |res| {
                        if let Ok(lobby_id) = res {
                            match tx.send(ChannelMessage::LobbyJoined(lobby_id)) {
                                Ok(_) => {}
                                Err(_) => {}
                            }
                        }
                    });
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
        if let Some(lobby_id) = steam_client.lobby_id {
            matchmaking
                .send_lobby_chat_message(lobby_id, &[0, 1, 2, 3, 4, 5])
                .expect("Failed to send chat message to lobby");
        }
    }
}
