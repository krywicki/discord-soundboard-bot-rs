//#![allow(warnings)]
use commands::PoiseResult;
use common::LogResult;
use db::{AudioTable, SettingsTable, Table};
use env_logger;
use log;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::Client as HttpClient;
use serenity::all::{
    ApplicationId, ComponentInteraction, ComponentInteractionDataKind, CreateInteractionResponse,
    FullEvent, Interaction, VoiceState,
};
use serenity::client::Context;

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
                    commands::scan(),
                    commands::register(),
                ],
                event_handler: |ctx, event, framework, data| {
                    Box::pin(event_handler(ctx, event, framework, data))
                },
                ..Default::default()
            })
            .setup(|_ctx, _ready, _framework| {
                Box::pin(async move {
                    //poise::builtins::register_globally(ctx, &framework.options().commands).await?;
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

async fn event_handler(
    ctx: &Context,
    event: &FullEvent,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    match event {
        FullEvent::Ready { data_about_bot } => {
            handle_ready(ctx, data_about_bot, framework, data).await?;
        }
        FullEvent::InteractionCreate { interaction } => {
            handle_interaction_create(ctx, interaction, framework, data).await?;
        }
        FullEvent::VoiceStateUpdate { old, new } => {
            handle_voice_state_update(ctx, old, new, framework, data).await?
        }
        _ => {}
    }

    Ok(())
}

async fn handle_ready(
    _ctx: &Context,
    ready: &serenity::model::gateway::Ready,
    _framework: FrameworkContext<'_>,
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
    SettingsTable::new(data.db_connection()).create_table();

    Ok(())
}

async fn handle_voice_state_update(
    ctx: &Context,
    old: &Option<VoiceState>,
    new: &VoiceState,
    _framework: FrameworkContext<'_>,
    _data: &UserData,
) -> PoiseResult {
    // Users with old.channel_id == None are joining a voice channel for the first time
    // Users with new.channel_id == None are leaving a voice channel
    // Users with old.channel_id == Some(_) and new.channel_id == Some(_) are moving from one voice channel to another
    match old {
        Some(VoiceState {
            guild_id: Some(old_guild_id),
            ..
        }) => {
            if helpers::is_bot_alone_in_voice_channel(&ctx, *old_guild_id).await? {
                log::info!(
                    "No one in voice channel. Bot is leaving. guild_id: {old_guild_id}, channel_id: {}",
                    new.channel_id.unwrap_or_default()
                );
                let manager = helpers::songbird_get(&ctx).await;
                manager.leave_voice_channel(*old_guild_id).await?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn handle_interaction_create(
    ctx: &Context,
    interaction: &Interaction,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    //log::debug!("interaction create event - {interaction:?}");
    match interaction {
        Interaction::Component(component) => {
            handle_component_interaction(ctx, interaction, component, framework, data).await?;
        }
        _ => {}
    }

    Ok(())
}

async fn handle_component_interaction(
    ctx: &Context,
    interaction: &Interaction,
    component: &ComponentInteraction,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!("component interaction event");
    match component.data.kind {
        ComponentInteractionDataKind::Button => {
            handle_btn_interaction(ctx, interaction, component, framework, data).await?
        }
        _ => {}
    }

    Ok(())
}

async fn handle_btn_interaction(
    ctx: &Context,
    interaction: &Interaction,
    component: &ComponentInteraction,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::debug!("Interaction Component Button pressed");
    let custom_id = &component.data.custom_id;

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await
        .log_err_msg("Failed to create response for btn interaction")
        .ok();

    match ButtonCustomId::try_from(custom_id.clone())? {
        ButtonCustomId::PlayAudio(audio_track_id) => {
            button_handlers::handle_play_audio_btn(
                ctx,
                interaction,
                component,
                framework,
                data,
                audio_track_id,
            )
            .await?;
        }
        ButtonCustomId::PlayRandom => {
            button_handlers::handle_play_random_btn(ctx, interaction, component, framework, data)
                .await?;
        }
        ButtonCustomId::DisplayAll => {
            button_handlers::handle_display_all_btn(ctx, interaction, component, framework, data)
                .await?;
        }
        ButtonCustomId::DisplayPinned => {
            button_handlers::handle_display_pinned_btn(
                ctx,
                interaction,
                component,
                framework,
                data,
            )
            .await?;
        }
        ButtonCustomId::DisplayRecentlyAdded => {
            button_handlers::handle_display_recently_added_btn(
                ctx,
                interaction,
                component,
                framework,
                data,
            )
            .await?;
        }
        ButtonCustomId::DisplayMostPlayed => {
            button_handlers::handle_display_most_played_btn(
                ctx,
                interaction,
                component,
                framework,
                data,
            )
            .await?;
        }
        ButtonCustomId::Unknown(value) => {
            return Err(format!(
                "Unrecognized button custom_id for component interaction. Value={value}"
            )
            .into())
            .log_err();
        }
    }

    Ok(())
}

pub mod button_handlers {
    use serenity::all::{CacheHttp, CreateMessage};

    use crate::{db::audio_table::AudioTableOrderBy, helpers::check_msg};

    use super::*;

    pub async fn handle_play_audio_btn(
        ctx: &Context,
        _interaction: &Interaction,
        component: &ComponentInteraction,
        _framework: FrameworkContext<'_>,
        data: &UserData,
        audio_track_id: i64,
    ) -> PoiseResult {
        log::info!("Play Audio Button Pressed - '{audio_track_id}'");

        let channel_id = component.channel_id;
        let guild_id = component
            .guild_id
            .ok_or("ComponentInteraction.guild_id is None")
            .log_err()?;

        let table = data.audio_table();

        match table.find_audio_row(db::UniqueAudioTableCol::Id(audio_track_id)) {
            Some(audio_row) => {
                log::info!(
                    "Found audio track. Name: {}, File: {}",
                    audio_row.name,
                    audio_row.audio_file.to_string_lossy()
                );

                let manager = helpers::songbird_get(&ctx).await;
                manager
                    .play_audio(guild_id, channel_id, &audio_row.audio_file)
                    .await
                    .ok();

                table.increment_play_count(audio_row.id)?;
            }
            None => {
                return Err(format!("Unable to locate audio track for button custom id").into())
                    .log_err();
            }
        }
        Ok(())
    }

    pub async fn handle_display_all_btn(
        ctx: &Context,
        _interaction: &Interaction,
        component: &ComponentInteraction,
        _framework: FrameworkContext<'_>,
        data: &UserData,
    ) -> PoiseResult {
        log::info!("Displaying all sounds buttons as ActionRows grid...");

        let channel_id = component.channel_id;

        let paginator = db::AudioTablePaginator::builder(data.db_connection())
            .order_by(AudioTableOrderBy::CreatedAt(db::Order::Desc))
            .page_limit(vars::ACTION_ROWS_LIMIT)
            .build();

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new().content("## Displaying All Sounds..."),
                )
                .await,
        );

        for audio_rows in paginator {
            let audio_rows = audio_rows.log_err()?;

            // ActionRows: Have a 5x5 grid limit
            // (https://discordjs.guide/message-components/action-rows.html#action-rows)
            let btn_grid: Vec<_> = audio_rows.chunks(5).map(helpers::make_action_row).collect();
            let builder = CreateMessage::new().components(btn_grid);
            check_msg(channel_id.send_message(&ctx.http(), builder).await);
        }

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new()
                        .content("### Sound Options")
                        .components(vec![helpers::make_display_buttons()]),
                )
                .await,
        );

        Ok(())
    }

    pub async fn handle_display_pinned_btn(
        ctx: &Context,
        _interaction: &Interaction,
        component: &ComponentInteraction,
        _framework: FrameworkContext<'_>,
        data: &UserData,
    ) -> PoiseResult {
        log::info!("Displaying pinned sounds buttons as ActionRows grid...");

        let channel_id = component.channel_id;

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new().content("## Displaying Pinned Sounds..."),
                )
                .await,
        );

        let paginator = db::AudioTablePaginator::builder(data.db_connection())
            .page_limit(vars::ACTION_ROWS_LIMIT)
            .pinned(Some(true))
            .order_by(AudioTableOrderBy::Name(db::Order::Asc))
            .build();

        for audio_rows in paginator {
            let audio_rows = audio_rows.log_err()?;

            // ActionRows: Have a 5x5 grid limit
            // (https://discordjs.guide/message-components/action-rows.html#action-rows)
            let btn_grid: Vec<_> = audio_rows.chunks(5).map(helpers::make_action_row).collect();
            let builder = CreateMessage::new().components(btn_grid);
            check_msg(channel_id.send_message(&ctx.http(), builder).await);
        }

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new()
                        .content("### Sound Options")
                        .components(vec![helpers::make_display_buttons()]),
                )
                .await,
        );

        Ok(())
    }

    pub async fn handle_display_recently_added_btn(
        ctx: &Context,
        _interaction: &Interaction,
        component: &ComponentInteraction,
        _framework: FrameworkContext<'_>,
        data: &UserData,
    ) -> PoiseResult {
        log::info!("Displaying recently added sounds buttons as ActionRows grid...");

        let channel_id = component.channel_id;

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new().content("## Displaying Recently Added Sounds..."),
                )
                .await,
        );

        let paginator = db::AudioTablePaginator::builder(data.db_connection())
            .page_limit(vars::ACTION_ROWS_LIMIT)
            .limit(Some(data.config.max_display_recently_added))
            .order_by(AudioTableOrderBy::CreatedAt(db::Order::Desc))
            .reverse(true)
            .build();

        for audio_rows in paginator {
            let audio_rows = audio_rows.log_err()?;

            // ActionRows: Have a 5x5 grid limit
            // (https://discordjs.guide/message-components/action-rows.html#action-rows)
            let btn_grid: Vec<_> = audio_rows.chunks(5).map(helpers::make_action_row).collect();
            let builder = CreateMessage::new().components(btn_grid);
            check_msg(channel_id.send_message(&ctx.http(), builder).await);
        }

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new()
                        .content("### Sound Options")
                        .components(vec![helpers::make_display_buttons()]),
                )
                .await,
        );

        Ok(())
    }

    pub async fn handle_display_most_played_btn(
        ctx: &Context,
        _interaction: &Interaction,
        component: &ComponentInteraction,
        _framework: FrameworkContext<'_>,
        data: &UserData,
    ) -> PoiseResult {
        log::info!("Displaying most played sounds buttons as ActionRows grid...");

        let channel_id = component.channel_id;

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new().content("## Displaying Most Played Sounds..."),
                )
                .await,
        );

        let paginator = db::AudioTablePaginator::builder(data.db_connection())
            .page_limit(vars::ACTION_ROWS_LIMIT)
            .limit(Some(data.config.max_display_most_played))
            .order_by(AudioTableOrderBy::PlayCount(db::Order::Desc))
            .build();

        for audio_rows in paginator {
            let audio_rows = audio_rows.log_err()?;

            // ActionRows: Have a 5x5 grid limit
            // (https://discordjs.guide/message-components/action-rows.html#action-rows)
            let btn_grid: Vec<_> = audio_rows.chunks(5).map(helpers::make_action_row).collect();
            let builder = CreateMessage::new().components(btn_grid);
            check_msg(channel_id.send_message(&ctx.http(), builder).await);
        }

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new()
                        .content("### Sound Options")
                        .components(vec![helpers::make_display_buttons()]),
                )
                .await,
        );

        Ok(())
    }

    pub async fn handle_play_random_btn(
        ctx: &Context,
        _interaction: &Interaction,
        component: &ComponentInteraction,
        _framework: FrameworkContext<'_>,
        data: &UserData,
    ) -> PoiseResult {
        log::info!("Play Random Button Pressed");

        let channel_id = component.channel_id;
        let guild_id = component
            .guild_id
            .ok_or("ComponentInteraction.guild_id is None")
            .log_err()?;
        let table = AudioTable::new(data.db_connection());
        let audio_row = table.get_random_row()?;

        match audio_row {
            Some(audio_row) => {
                let track_name = audio_row.name;

                check_msg(
                    channel_id
                        .send_message(
                            &ctx.http(),
                            CreateMessage::new()
                                .content(format!("### Playing `{track_name}`..."))
                                .components(vec![helpers::make_display_buttons()]),
                        )
                        .await,
                );

                let manager = helpers::songbird_get(&ctx).await;
                manager
                    .play_audio(guild_id, channel_id, &audio_row.audio_file)
                    .await
                    .ok();
            }
            None => check_msg(
                channel_id
                    .send_message(
                        &ctx.http(),
                        CreateMessage::new()
                            .content("No sounds present to play")
                            .components(vec![helpers::make_display_buttons()]),
                    )
                    .await,
            ),
        }

        Ok(())
    }
}
