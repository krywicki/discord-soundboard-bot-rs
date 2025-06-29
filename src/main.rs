//#![allow(warnings)]
use env_logger;
use log;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::Client as HttpClient;
use serenity::all::ApplicationId;

use serenity::{
    client::Client,
    prelude::{GatewayIntents, TypeMapKey},
};

use songbird::SerenityInit;

mod audio;
mod commands;
mod common;
mod config;
mod db;
mod errors;
mod event_handlers;
mod helpers;
mod vars;

use crate::commands::PoiseError;
use crate::common::UserData;
use crate::config::Config;

type FrameworkContext<'a> = poise::FrameworkContext<'a, UserData, PoiseError>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Application starting...");

    let config = Config::new();
    env_logger::init();

    // framework configuration
    let token = config.token.clone();
    let cmd_prefix = config.command_prefix.clone();
    let application_id = config.application_id;
    let sqlite_db_file = config.sqlite_db_file.clone();
    let db_manager = SqliteConnectionManager::file(sqlite_db_file);
    let db_pool = r2d2::Pool::new(db_manager).expect("Failed to create sqlite connection pool");

    log::info!("Setting up framework...");
    let framework: poise::Framework<UserData, PoiseError> =
        poise::Framework::<UserData, PoiseError>::builder()
            .options(poise::FrameworkOptions {
                prefix_options: poise::PrefixFrameworkOptions {
                    prefix: Some(cmd_prefix),
                    ..Default::default()
                },
                commands: vec![
                    commands::join(),
                    commands::leave(),
                    commands::sounds(),
                    commands::play(),
                    commands::tts(),
                    commands::register(),
                ],
                event_handler: |ctx, event, framework, data| {
                    Box::pin(event_handlers::event_handler(ctx, event, framework, data))
                },
                ..Default::default()
            })
            .setup(|_ctx, _ready, _framework| {
                Box::pin(async move {
                    Ok(UserData {
                        config: config,
                        db_pool: db_pool,
                    })
                })
            })
            .build();

    // client setup
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILDS;

    log::info!("Creating client...");
    let mut client = Client::builder(&token, intents)
        .application_id(ApplicationId::new(application_id))
        .framework(framework)
        .register_songbird()
        .type_map_insert::<HttpKey>(HttpClient::new())
        .await
        .expect("Error creating client");

    // run client
    log::info!("Running client...");
    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|why| println!("Client ended: {:?}", why));
    });

    tokio::signal::ctrl_c().await.ok();
    log::info!("Received Ctrl-C, shutting down.");

    Ok(())
}

pub struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}
