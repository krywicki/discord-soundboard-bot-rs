use poise::{command, Modal};
use serenity::{
    all::{
        CreateActionRow, CreateButton, CreateInteractionResponse,
        CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateMessage,
        Message,
    },
    async_trait,
};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};

use crate::{
    audio::{self, play_audio_track, AudioFile, RemoveAudioFile, TrackHandleHelper},
    common::{LogResult, UserData},
    config::Config,
    db::{self, AudioTable, AudioTableRowInsert, FtsText},
    helpers::{
        self, check_msg, poise_check_msg, poise_songbird_get, ButtonCustomId, ButtonLabel,
        PoiseContextHelper, SongbirdHelper,
    },
    vars,
};

pub type GenericError = Box<dyn std::error::Error + Send + Sync>;
pub type PoiseError = GenericError;
pub type PoiseContext<'a> = poise::Context<'a, UserData, PoiseError>;
pub type PoiseResult = Result<(), PoiseError>;
pub type PoiseAppContext<'a> = poise::ApplicationContext<'a, UserData, PoiseError>;

#[poise::command(prefix_command, guild_only)]
pub async fn deafen(ctx: PoiseContext<'_>) -> PoiseResult {
    Ok(())
}

#[poise::command(prefix_command, guild_only)]
pub async fn ping(ctx: PoiseContext<'_>) -> PoiseResult {
    poise_check_msg(ctx.say("pong!").await);
    Ok(())
}

#[poise::command(prefix_command, guild_only)]
pub async fn join(ctx: PoiseContext<'_>) -> PoiseResult {
    log::info!("Bot joining voice channel...");
    let (guild_id, connect_to) = helpers::get_author_voice_channel(&ctx)?;

    log::info!("Bot will join Guild ID: {guild_id}, Voice Channel: {connect_to}");
    let manager = helpers::poise_songbird_get(&ctx).await;

    match manager.join(guild_id, connect_to).await {
        Ok(handler_lock) => {
            // Attach an event handler to see notifications of all track errors
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
            log::info!("Bot joined Guild ID: {guild_id}, Voice Channel ID: {connect_to}");
        }
        Err(err) => {
            log::error!(
                "Bot failed to join Guild ID: {guild_id}, Voice Channel ID: {connect_to} - {}",
                err.to_string()
            )
        }
    }

    if let Some(ref join_audio) = ctx.data().config.join_audio {
        log::debug!("bot join audio enabled - song: {}", join_audio);
        audio::play_audio_track(manager, guild_id, connect_to, &join_audio).await?;
    }
    Ok(())
}

#[poise::command(prefix_command, guild_only)]
pub async fn leave(ctx: PoiseContext<'_>) -> PoiseResult {
    let manager = helpers::poise_songbird_get(&ctx).await;
    let guild_id = ctx
        .guild_id()
        .ok_or("command::leave - Failed to get guild_id")?;

    let handler = manager.get(guild_id);
    let channel_id = ctx.channel_id();

    match handler {
        Some(handler) => {
            // if leave audio set, play exit audio track
            if let Some(ref leave_audio) = ctx.data().config.leave_audio {
                log::debug!("bot leave audio enabled - song: {}", leave_audio);
                play_audio_track(manager.clone(), guild_id, channel_id, &leave_audio)
                    .await?
                    .wait_for_end()
                    .await;
            }

            match manager.remove(guild_id).await {
                Ok(_) => poise_check_msg(ctx.say("Left voice channel").await),
                Err(e) => poise_check_msg(ctx.say(format!("Failed {:?}", e)).await),
            }
        }
        None => poise_check_msg(ctx.reply("Not in a voice channel").await),
    }

    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn play(
    ctx: PoiseContext<'_>,
    #[rename = "track"]
    #[description = "Track to play"]
    #[autocomplete = "helpers::autocomplete_audio_track_name"]
    audio_track_name: String,
) -> PoiseResult {
    log::info!("Playing audio track {audio_track_name}...");

    let table = ctx.data().audio_table();
    let guild_id = ctx.guild_id().ok_or("No guild id found")?;
    let channel_id = ctx.channel_id();
    let manager = ctx.songbird().await;

    let row = table.find_audio_row(db::UniqueAudioTableCol::Name(audio_track_name.clone()));
    match row {
        Some(row) => {
            poise_check_msg(ctx.reply(format!("Playing track {audio_track_name}")).await);
            manager
                .play_audio(guild_id, channel_id, &row.audio_file)
                .await?;
        }
        None => poise_check_msg(
            ctx.reply(format!("Audio Track '{audio_track_name}' not found"))
                .await,
        ),
    }

    Ok(())
}

// #[poise::command(prefix_command, guild_only)]
// async fn list(ctx: PoiseContext<'_>, msg: &Message) -> PoiseResult {
//     let audio_tracks_md = audio::list_audio_track_names_markdown();

//     helpers::check_msg(msg.reply(ctx, audio_tracks_md).await);

//     Ok(())
// }

#[poise::command(slash_command, prefix_command, guild_only, subcommands("add_sound"))]
pub async fn sounds(ctx: PoiseContext<'_>) -> PoiseResult {
    log::info!("List sounds buttons as ActionRows grid...");
    let paginator = db::AudioTablePaginator::builder(ctx.data().db_connection())
        .page_limit(vars::ACTION_ROWS_LIMIT)
        .build();

    for audio_rows in paginator {
        let audio_rows = audio_rows.log_err()?;
        let mut action_grid: Vec<Vec<CreateActionRow>> = vec![];

        // ActionRows: Have a 5x5 grid limit
        // (https://discordjs.guide/message-components/action-rows.html#action-rows)
        let btn_grid: Vec<_> = audio_rows.chunks(5).map(helpers::make_action_row).collect();
        let builder = CreateMessage::new().components(btn_grid);
        check_msg(ctx.channel_id().send_message(&ctx.http(), builder).await);
    }

    Ok(())
}

// #[poise::command(prefix_command, guild_only)]
// async fn help(ctx: PoiseContext<'_>, msg: &Message) -> PoiseResult {
//     Ok(())
// }

#[poise::command(prefix_command, guild_only)]
pub async fn scan(ctx: PoiseContext<'_>) -> PoiseResult {
    log::info!("Scanning audio files...");

    let mut audio_files: Vec<AudioFile> = ctx.data().read_audio_dir().into_iter().collect();
    let paginator = db::AudioTablePaginator::builder(ctx.data().db_connection()).build();

    // ignore audio files already in database
    for page in paginator {
        let page = page.log_err()?;
        for row in page {
            audio_files.remove_audio_file(&row.audio_file);
        }
    }

    // add remaining audio files not in database
    log::info!(
        "Scan found {} audio files to add to databse",
        audio_files.len()
    );
    let mut inserted = 0;
    let table = AudioTable::new(ctx.data().db_connection());
    for audio_file in audio_files {
        let audio_name = audio_file.audio_title();

        let new_audio = AudioTableRowInsert {
            name: audio_name.clone(),
            tags: audio_file.file_stem().fts_clean(),
            audio_file: audio_file,
            created_at: chrono::Utc::now(),
            author_id: None,
            author_name: None,
            author_global_name: None,
        };

        table
            .insert_audio_row(new_audio)
            .log_err()
            .and_then(|val| {
                inserted += 1;
                Ok(())
            })
            .ok();
    }

    log::info!("Scan complete - added {inserted} new audio files");
    Ok(())
}

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn echo(
    ctx: PoiseContext<'_>,
    #[description = "Text to echo back"] text: Option<String>,
) -> PoiseResult {
    let response = format!("Echo: '{}'", text.unwrap_or("".into()));
    poise_check_msg(ctx.say(response).await);

    Ok(())
}

#[poise::command(prefix_command, guild_only)]
pub async fn register(ctx: PoiseContext<'_>) -> PoiseResult {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[derive(Debug, poise::Modal)]
#[name = "Add Sound"]
struct AddSoundModal {
    #[name = "Name"] // Field name by default
    #[placeholder = "Use The Force Luke"] // No placeholder by default
    #[min_length = 5] // No length restriction by default (so, 1-4000 chars)
    #[max_length = 500]
    name: String,

    #[name = "Tags"] // Field name by default
    #[placeholder = "star Wars jedi luke skywalker force episode iv"] // No placeholder by default
    #[min_length = 3] // No length restriction by default (so, 1-4000 chars)
    #[max_length = 1024]
    tags: Option<String>,

    #[name = "Audio URL"]
    #[placeholder = "www.example.com/use-the-force.mp3"]
    #[max_length = 2048]
    url: String,
}

#[poise::command(slash_command, guild_only, rename = "add")]
pub async fn add_sound(ctx: PoiseAppContext<'_>) -> PoiseResult {
    let data = AddSoundModal::execute(ctx)
        .await?
        .ok_or("AddSoundModal not set")
        .log_err()?;

    log::info!("Adding sound. Name: {}, Url: {}", data.name, data.url);

    let table = ctx.data.audio_table();
    let row = table.find_audio_row(db::UniqueAudioTableCol::Name(data.name.clone()));

    match row {
        Some(_) => {
            log::error!(
                "Can't add sound. Sound already exists - name: {}",
                data.name
            );
            poise_check_msg(ctx.reply("A sound by that name already exists").await);
        }
        None => {
            let audio_file =
                helpers::download_audio_url(&data.url, &ctx.data.config.audio_dir.as_path())
                    .await?;

            table
                .insert_audio_row(AudioTableRowInsert {
                    name: data.name.clone(),
                    audio_file: audio_file,
                    author_global_name: ctx.author().global_name.clone(),
                    author_id: Some(ctx.author().id.into()),
                    author_name: Some(ctx.author().name.clone()),
                    tags: format!("{} {}", data.name, data.tags.unwrap_or("".into())).fts_clean(),
                    created_at: chrono::Utc::now(),
                })
                .log_err()?;
        }
    }

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
