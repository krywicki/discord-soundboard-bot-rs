#![allow(warnings)]
use std::path::{self, Path};
use std::sync::Arc;
use std::{env, fs};

use commands::{PoiseContext, PoiseResult};
use db::{AudioTable, Table};
use env_logger;
use log;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::Client as HttpClient;
use serenity::all::{
    ApplicationId, ChannelId, ComponentInteraction, ComponentInteractionDataKind, CreateActionRow,
    CreateButton, CreateEmbed, CreateInteractionResponse, CreateMessage, Embed, FullEvent, GuildId,
    Interaction,
};
use serenity::client::Context;
use serenity::json::to_string;
use serenity::model::channel;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::{
        standard::{
            macros::{command, group},
            Args, CommandResult, Configuration,
        },
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready},
    prelude::{GatewayIntents, TypeMapKey},
    Result as SerenityResult,
};
use songbird::events::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};
use songbird::tracks::{PlayMode, TrackHandle, TrackState};
use songbird::SerenityInit;

mod audio;
mod commands;
mod common;
mod config;
mod db;
mod errors;
mod helpers;
mod vars;

use crate::commands::PoiseError;
use crate::common::UserData;
use crate::config::Config;
use crate::helpers::ButtonCustomId;
use crate::helpers::SongbirdHelper;

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
                    commands::echo(),
                    commands::join(),
                    commands::leave(),
                    commands::sounds(),
                    commands::play(),
                ],
                event_handler: |ctx, event, framework, data| {
                    Box::pin(event_handler(ctx, event, framework, data))
                },
                ..Default::default()
            })
            .setup(|ctx, _ready, framework| {
                Box::pin(async move {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    Ok(UserData {
                        config: config,
                        db_pool: db_pool,
                    })
                })
            })
            .build();

    // client setup
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

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

async fn event_handler(
    ctx: &Context,
    event: &FullEvent,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    match event {
        FullEvent::Ready { data_about_bot } => {
            handle_ready(ctx, data_about_bot, framework, data).await?
        }
        FullEvent::InteractionCreate { interaction } => {
            handle_interaction_create(ctx, interaction, framework, data).await?
        }
        _ => {}
    }

    Ok(())
}

async fn handle_ready(
    ctx: &Context,
    ready: &serenity::model::gateway::Ready,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!(
        "Ready info...\
            \n\t User Name: {user_name} \
            \n\t User Id: {user_id} \
            \n\t Is Bot: {is_bot} \
            \n\t Session Id: {session_id} \
            \n\t Version: {version} \
            ",
        user_name = ready.user.name,
        user_id = ready.user.id,
        is_bot = ready.user.bot,
        session_id = ready.session_id,
        version = ready.version
    );

    AudioTable::new(data.db_connection()).create_table();

    Ok(())
}

async fn handle_interaction_create(
    ctx: &Context,
    interaction: &Interaction,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    match interaction {
        Interaction::Component(component) => match component.data.kind {
            ComponentInteractionDataKind::Button => {
                log::debug!("Interaction Component Button pressed");
                let custom_id = &component.data.custom_id;
                match helpers::ButtonCustomId::try_from(custom_id.clone()) {
                    Ok(custom_id) => match custom_id {
                        helpers::ButtonCustomId::PlayAudio(audio_track) => {
                            log::debug!("Play Audio Button Pressed - '{}'", audio_track);
                            let audio_track = audio_track.as_str();
                            let channel_id = component.channel_id;
                            let guild_id = component.guild_id.unwrap();

                            let manager = helpers::songbird_get(ctx).await;
                            // manager
                            //     .play_audio(guild_id, channel_id, audio_track)
                            //     .await?;
                        }
                        _ => {
                            log::error!("ButtonCustomId not handled! - {:?}", custom_id);
                        }
                    },
                    Err(err) => {
                        log::error!("Failed to parse custom id - {}", err);
                        helpers::check_msg(
                            component
                                .channel_id
                                .say(&ctx.http, "Sorry. I don't recognize this button.")
                                .await,
                        );
                    }
                }

                component
                    .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
                    .await;
            }

            _ => {}
        },
        _ => {}
    }

    Ok(())
}

async fn handle_component_interaction(
    ctx: &Context,
    interaction: &ComponentInteraction,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    match interaction.data.kind {
        ComponentInteractionDataKind::Button => {
            handle_btn_interaction(ctx, interaction, framework, data).await?
        }
        _ => {}
    }

    Ok(())
}

async fn handle_btn_interaction(
    ctx: &Context,
    interaction: &ComponentInteraction,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::debug!("Interaction Component Button pressed");
    let custom_id = &interaction.data.custom_id;

    match ButtonCustomId::from(custom_id.clone()) {
        ButtonCustomId::PlayAudio(audio_track) => {
            log::debug!("Play Audio Button Pressed - '{}'", audio_track);

            let channel_id = interaction.channel_id;
            let guild_id = interaction
                .guild_id
                .ok_or("ComponentInteraction.guild_id is None")?;

            let manager = helpers::songbird_get(&ctx).await;
            audio::play_audio_track(manager, guild_id, channel_id, audio_track).await;
        }
        ButtonCustomId::Unknown(value) => {
            let err =
                format!("Unrecognized button custom_id for component interaction. Value={value}");
            log::error!("{err}");
            return Err(err.into());
        }
    }

    Ok(())
}
