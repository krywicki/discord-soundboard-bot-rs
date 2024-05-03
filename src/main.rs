#![allow(warnings)]
use std::path::{self, Path};
use std::sync::Arc;
use std::{env, fs};

use env_logger;
use log;
use reqwest::Client as HttpClient;
use serenity::all::{
    ApplicationId, CreateActionRow, CreateButton, CreateEmbed, CreateMessage, Embed, GuildId,
    Interaction,
};
use serenity::client::Context;
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
use songbird::SerenityInit;
use symphonia::core::audio;

struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Application starting...");

    // dotenv file and log init
    let env_file = env::var("DISCORD_SOUNDBOARD_BOT_DOTENV_FILE").unwrap_or(".env".into());
    let dotenv_loaded = dotenv::from_filename(env_file.as_str()).is_ok();
    env_logger::init();

    if dotenv_loaded {
        log::info!("dotenv file '{env_file}' loaded");
    }

    // framework configuration
    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN in environment");
    let prefix = env::var("DISCORD_SOUNDBOARD_BOT_COMMAND_PREFIX").unwrap_or("sb:".into());

    let framework = StandardFramework::new().group(&GENERAL_GROUP);
    framework.configure(Configuration::new().prefix(prefix.as_str()));

    // client setup
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;
    let application_id: u64 = env::var("DISCORD_APPLICATION_ID")
        .expect("Missing DISCORD_APPLICATION_ID env var")
        .parse()?;

    let mut client = Client::builder(&token, intents)
        .application_id(ApplicationId::new(application_id))
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .type_map_insert::<HttpKey>(HttpClient::new())
        .await
        .expect("Error creating client");

    // run client
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

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
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
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Component(component) => {}
            _ => {}
        }
    }
}

#[group]
#[commands(ping, join, leave, play, list, sounds)]
struct General;

#[command]
#[only_in(guilds)]
async fn deafen(ctx: &Context, msg: &Message) -> CommandResult {
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    check_msg(msg.channel_id.say(&ctx.http, "pong!").await);
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    // get author guild id and channel id
    let (guild_id, channel_id) = {
        let guild = msg.guild(&ctx.cache).unwrap();
        let channel_id = guild
            .voice_states
            .get(&msg.author.id)
            .and_then(|voice_state| voice_state.channel_id);

        (guild.id, channel_id)
    };

    // ignore if author not in voice channel
    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            log::info!("Can't join, author({}) not in voice channel", msg.author.id);
            check_msg(msg.reply(ctx, "You are not in a voice channel").await);
            return Ok(());
        }
    };

    let manager = songbird_get(&ctx).await;

    match manager.join(guild_id, connect_to).await {
        Ok(handler_lock) => {
            // Attach an event handler to see notifications of all track errors
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
            log::info!(
                "Bot joined GuildId:{}, VoiceChannelId:{}",
                guild_id,
                connect_to
            );
        }
        Err(err) => {
            log::error!(
                "Bot failed to join GuildId:{}, VoiceChannelId:{} - {}",
                guild_id,
                connect_to,
                err.to_string()
            )
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild_id = msg.guild_id.unwrap();
    let manager = songbird_get(&ctx).await;

    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        match manager.remove(guild_id).await {
            Ok(_) => check_msg(msg.channel_id.say(&ctx.http, "Left voice channel").await),
            Err(e) => {
                check_msg(
                    msg.channel_id
                        .say(&ctx.http, format!("Failed {:?}", e))
                        .await,
                );
            }
        }
    } else {
        check_msg(msg.reply(ctx, "Not in a voice channel").await);
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let guild_id = msg.guild_id.unwrap();
    let manager = songbird_get(&ctx).await;

    let audio_track_name = match args.single::<String>() {
        Ok(name) => name.trim().to_string(),
        Err(_) => {
            log::error!("Missing audio track name in play command");
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide audio track name to play")
                    .await,
            );
            return Ok(());
        }
    };

    match manager.get(guild_id) {
        Some(handler_lock) => {
            let mut handler = handler_lock.lock().await;

            match find_audio_track(&audio_track_name) {
                Some(audio_track_input) => {
                    handler.play_input(audio_track_input.into());

                    check_msg(msg.channel_id.say(&ctx.http, "Playing track").await);
                    log::info!("Playing track {}", audio_track_name);
                }
                None => {
                    log::error!("Audio track does not exist - {}", audio_track_name);
                    check_msg(
                        msg.channel_id
                            .say(
                                &ctx.http,
                                format!("Cannot locate audio track: '{}'", audio_track_name),
                            )
                            .await,
                    );
                }
            }
        }
        None => {}
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn list(ctx: &Context, msg: &Message) -> CommandResult {
    let audio_tracks_md = list_audio_track_names_markdown();

    check_msg(msg.reply(ctx, audio_tracks_md).await);

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn sounds(ctx: &Context, msg: &Message) -> CommandResult {
    let audio_tracks = list_audio_track_names();

    let buttons = vec![CreateButton::new(&audio_tracks[0]).label(&audio_tracks[0])];
    let buttons = CreateActionRow::Buttons(buttons);

    let mut embed = CreateEmbed::new().title("Soundboard Sounds").field(
        "embed field",
        "embed field value",
        true,
    );

    let builder = CreateMessage::new()
        .add_embed(embed)
        .components(vec![buttons]);
    check_msg(msg.channel_id.send_message(&ctx.http, builder).await);

    //embed.check_msg(msg.reply(ctx, embed.into()).await);

    //check_msg(msg.reply(ctx, audio_tracks_md).await);

    Ok(())
}

struct TrackErrorNotifier;

#[async_trait]
impl VoiceEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                log::error!(
                    "Track {:?} encountered an error: {:?}",
                    handle.uuid(),
                    state.playing
                );
            }
        }

        None
    }
}

async fn songbird_get(ctx: &Context) -> Arc<songbird::Songbird> {
    songbird::get(ctx)
        .await
        .expect("Songbird voice client placed in at initialization")
        .clone()
}

/// check if message successfully sent, or log to error
fn check_msg(result: SerenityResult<Message>) {
    if let Err(err) = result {
        log::error!("Error sending message: {:?}", err);
    }
}

fn find_audio_track(name: &String) -> Option<songbird::input::File<impl AsRef<Path>>> {
    let audio_dir = env::var("DISCORD_SOUNDBOARD_BOT_AUDIO_DIR").unwrap_or("./audio".into());
    let audio_file = format!("{}.mp3", name);

    let audio_file_path = path::Path::new(&audio_dir).join(&audio_file);

    log::debug!(
        "Looking for audio track: {}",
        audio_file_path.to_str().unwrap_or("")
    );
    if audio_file_path.exists() {
        Some(songbird::input::File::new(audio_file_path))
    } else {
        None
    }
}

fn list_audio_track_names() -> Vec<String> {
    let audio_dir = env::var("DISCORD_SOUNDBOARD_BOT_AUDIO_DIR").unwrap_or("./audio".into());

    log::debug!("DISCORD_SOUNDBOARD_BOT_AUDIO_DIR: {audio_dir}");

    let audio_tracks: Vec<String> = match fs::read_dir(&audio_dir) {
        Ok(entries) => {
            let mut tracks: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| {
                    let p = e.path();
                    let os_path = p.file_name().unwrap();
                    os_path.to_str().unwrap().to_string()
                })
                .filter(|e| e.ends_with(".mp3"))
                .map(|e| e.strip_suffix(".mp3").unwrap().to_string())
                //.map(|e| String::from(e.to_str().unwrap()))
                .collect::<Vec<String>>();
            tracks.sort();
            tracks
        }
        Err(err) => {
            log::error!("Failed to read audio tracks at dir: {audio_dir}");
            vec![]
        }
    };

    log::debug!("Audio tracks: {:?}", audio_tracks);

    audio_tracks
}

fn list_audio_track_names_markdown() -> String {
    let audio_track_names = list_audio_track_names();
    let command_prefix = env::var("DISCORD_SOUNDBOARD_BOT_COMMAND_PREFIX").unwrap_or("sb:".into());

    let audio_tracks_md = audio_track_names
        .iter()
        .map(|track| format!("- {command_prefix}play {track}\n"))
        .collect();

    audio_tracks_md
}
