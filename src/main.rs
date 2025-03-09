use bevy::{prelude::*, window::PrimaryWindow};
use steamworks::{
    networking_types::{NetworkingIdentity, SendFlags},
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
}

impl SteamClient {
    pub fn new() -> SteamClient {
        SteamClient {
            client: Client::init_app(480).unwrap(),
        }
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
    q_camera: Query<(&Camera, &GlobalTransform)>,
) {
    steam_client.client.run_callbacks();

    let (camera, camera_transform) = q_camera.single();
    let window = q_window.single();

    if let Some(position) = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor).ok())
        .map(|ray| ray.origin.truncate())
    {
        gizmos.circle_2d(Isometry2d::from_translation(position), 10., Color::WHITE);

        for friend in steam_client
            .client
            .friends()
            .get_friends(FriendFlags::IMMEDIATE)
        {
            let identity = NetworkingIdentity::new_steam_id(friend.id());

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
        println!("received message");
        let peer = message.identity_peer();
        let data = message.data();

        let peer_x = f32::from_le_bytes(data[0..4].try_into().expect("Someone sent bad message"));
        let peer_y = f32::from_le_bytes(data[4..8].try_into().expect("Someone sent bad message"));

        peers.push((peer.debug_string(), Vec2::new(peer_x, peer_y)));
    }

    for peer in peers {
        gizmos.circle_2d(Isometry2d::from_translation(peer.1), 10., Color::WHITE);
    }
}
