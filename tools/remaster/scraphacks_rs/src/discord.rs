use std::{num::NonZeroU32, thread::JoinHandle, time::SystemTime};

use crate::{cdbg, ceprintln, cprint, cprintln};
use anyhow::{bail, Result};
use discord_sdk::{
    activity::{ActivityBuilder, Assets, PartyPrivacy, Secrets},
    registration::{register_app, Application, LaunchCommand},
    wheel::Wheel,
    Discord, DiscordApp, Subscriptions,
};
const APP_ID: discord_sdk::AppId = 1066820570097930342;
const STEAM_APP_ID: u32 = 897610;
pub struct Client {
    pub discord: discord_sdk::Discord,
    pub user: discord_sdk::user::User,
    pub wheel: discord_sdk::wheel::Wheel,
}

impl Client {
    pub fn run() -> Result<JoinHandle<Result<()>>> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        register_app(Application {
            id: APP_ID,
            name: Some("Scrapland Remastered".to_owned()),
            command: LaunchCommand::Steam(STEAM_APP_ID),
        })?;
        Ok(std::thread::spawn(move || rt.block_on(Self::run_async())))
    }
    async fn run_async() -> Result<()> {
        let (wheel, handler) = Wheel::new(Box::new(|err| {
            ceprintln!("Encountered an error: {}", err);
        }));
        let mut user = wheel.user();
        let discord = Discord::new(
            DiscordApp::PlainId(APP_ID),
            Subscriptions::ACTIVITY,
            Box::new(handler),
        )?;
        user.0.changed().await?;
        let user = match &*user.0.borrow() {
            discord_sdk::wheel::UserState::Connected(user) => user.clone(),
            discord_sdk::wheel::UserState::Disconnected(err) => {
                ceprintln!("Failed to connect to Discord: {err}");
                bail!("{}", err);
            }
        };
        let uid = user.id;
        cprintln!(
            "Logged in as: {user}#{discriminator}",
            user = user.username,
            discriminator = user
                .discriminator
                .map(|d| d.to_string())
                .unwrap_or_else(|| "????".to_owned())
        );
        let mut activity = ActivityBuilder::new()
            .state("Testing")
            .assets(Assets::default().large("scrap_logo", Some("Testing")))
            .timestamps(Some(SystemTime::now()), Option::<SystemTime>::None)
            .details("Testing ScrapHack");
        if false {
            // (SCRAP.is_server()||SCRAP.is_client())
            let players = 1;
            let capacity = 32;
            activity = activity
                .instance(true)
                .party(
                    "Testt",
                    NonZeroU32::new(players),
                    NonZeroU32::new(capacity),
                    if false {
                        PartyPrivacy::Private
                    } else {
                        PartyPrivacy::Public
                    }
                )
                .secrets(Secrets {
                    r#match: Some("MATCH".to_owned()),     // Use server_ip+port
                    join: Some("JOIN".to_owned()),         // Use server_ip+port
                    spectate: Some("SPECTATE".to_owned()), // Use server_ip+port
                });
        }

        discord.update_activity(activity).await?;
        loop {
            if let Ok(req) = wheel.activity().0.try_recv() {
                cprintln!("Got Join request: {req:?}");
            }
        }
        Ok(())
    }
}
