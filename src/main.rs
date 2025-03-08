use bevy::prelude::*;
use crossbeam_channel::*;
use steamworks::*;

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
    create_lobby_channel: SteamChannel<LobbyId>,
    lobby_chat_channel: SteamChannel<LobbyChatMsg>,
}

impl SteamClient {
    pub fn new() -> SteamClient {
        SteamClient {
            client: Client::init_app(480).unwrap(),
            create_lobby_channel: SteamChannel::new(),
            lobby_chat_channel: SteamChannel::new(),
        }
    }
}

struct SteamChannel<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T> SteamChannel<T> {
    pub fn new() -> SteamChannel<T> {
        let (sender, receiver) = unbounded();
        SteamChannel { sender, receiver }
    }
}

fn setup(client: Res<SteamClient>) {
    let sender = client.lobby_chat_channel.sender.clone();

    // Register the callback with the cloned sender
    client
        .client
        .register_callback(move |message: LobbyChatMsg| {
            println!("Lobby chat message received: {:?}", message);
            sender.send(message).unwrap();
        });
}

fn update(keys: Res<ButtonInput<KeyCode>>, steam_client: Res<SteamClient>) {
    steam_client.client.run_callbacks();
    let matchmaking = steam_client.client.matchmaking();
    let tx = steam_client.create_lobby_channel.sender.clone();

    if keys.just_pressed(KeyCode::KeyC) {
        matchmaking.create_lobby(LobbyType::Private, 4, move |result| match result {
            Ok(lobby_id) => {
                tx.send(lobby_id).unwrap();
                println!("Created lobby: [{}]", lobby_id.raw())
            }
            Err(err) => panic!("Error: {}", err),
        });
    }

    if keys.just_pressed(KeyCode::KeyT) {}

    if let Ok(lobby_id) = steam_client.create_lobby_channel.receiver.try_recv() {
        println!("Sending message to lobby chat...");
        matchmaking
            .send_lobby_chat_message(lobby_id, &[0, 1, 2, 3, 4, 5])
            .expect("Failed to send chat message to lobby");
    }

    if let Ok(message) = steam_client.lobby_chat_channel.receiver.try_recv() {
        let mut buffer = vec![0; 256];
        let buffer =
            matchmaking.get_lobby_chat_entry(message.lobby, message.chat_id, buffer.as_mut_slice());
        println!("Message buffer: [{:?}]", buffer);
    }
}
