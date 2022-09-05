use std::{env, process::exit};

use matrix_sdk::{
	self,
	config::SyncSettings,
	room::Room,
	ruma::events::room::{
		member::{
			StrippedRoomMemberEvent,
			SyncRoomMemberEvent,
			MembershipState,
		},
	},
	ruma::api::client::room::create_room::v3::Request as CreateRoomRequest,
	Client,
};
use url::Url;
use tokio::time::{sleep, Duration};

async fn on_stripped_state_member(
	room_member: StrippedRoomMemberEvent,
	client: Client,
	room: Room,
) {
	if room_member.state_key != client.user_id().await.unwrap() {
		return;
	}

	if let Room::Invited(room) = room {
		tokio::spawn(async move {
			println!("Autojoining room {}", room.room_id());
			let mut delay = 2;

			while let Err(err) = room.accept_invitation().await {
				// retry autojoin due to synapse sending invites, before the
				// invited user can join for more information see
				// https://github.com/matrix-org/synapse/issues/4345
				eprintln!("Failed to join room {} ({err:?}), retrying in {delay}s", room.room_id());

				sleep(Duration::from_secs(delay)).await;
				delay *= 2;

				if delay > 3600 {
					eprintln!("Can't join room {} ({err:?})", room.room_id());
					break;
				}
			}
			println!("Successfully joined room {}", room.room_id());
		});
	}
}


async fn on_new_member(
    room_member: SyncRoomMemberEvent,
    client: Client,
    room: Room,
) {
	// Skip old events
	if room_member.origin_server_ts().to_system_time().unwrap().elapsed().unwrap() > Duration::from_secs(10) {
		return;
	}
	let sender = room_member.sender();
	if sender == client.user_id().await.unwrap() {
		return;
	}

	if room_member.membership() == &MembershipState::Join {
		if let Room::Joined(room) = room {
			match room.kick_user(
				&sender,
				Some("Open DM for verification")
			).await {
				Ok(_) => {},
				Err(err) => {
					eprintln!("Cannot kick {}, error {}", sender, err);
					return;
				}
			}
			println!("Creating DM room for user {}", sender);
			let request = CreateRoomRequest::new();
			let dm = match client.create_room(request).await {
					Ok(room) => client.get_room(&room.room_id),
					Err(err) => {
						println!("Error in first match {}", err);
						return;
					},
			};
			println!("IT FUCKING WORKS. {:?}", dm);
			// println!("Inviting user {} to room {}", sender, dm.room_id());
			// match dm.invite_user_by_id(&sender).await {
			// 		Ok(_) => {
			// 			println!("Invited user!");
			// 		},
			// 		Err(err) => {
			// 			eprintln!("Cannot invite {} to captcha room, error {}", sender, err);
			// 		},
			// }
		}
	}
}

async fn login(homeserver_url: String, username: &str, password: &str) -> matrix_sdk::Result<()> {
	let homeserver_url = Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
	let client = Client::new(homeserver_url).await.unwrap();

	client.register_event_handler(on_stripped_state_member).await;
	client.register_event_handler(on_new_member).await;

	client.login(username, password, Some("captcha"), Some("captcha")).await.unwrap();
	client.sync(SyncSettings::new()).await;

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	tracing_subscriber::fmt::init();

	let (homeserver_url, username, password) =
		match (env::args().nth(1),
				env::var("MX_USERNAME").expect("MX_USERNAME is unset"),
				env::var("MX_PASSWORD").expect("MX_PASSWORD is unser")) {
			(Some(a), b, c) => (a, b, c),
			_ => {
				eprintln!(
					"Usage: {} <homeserver_url>",
					env::args().next().unwrap()
				);
				exit(1)
			}
		};

	login(homeserver_url, &username, &password).await?;

	Ok(())
}
