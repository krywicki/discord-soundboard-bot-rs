use poise::command;
use serenity::{
    all::{CreateActionRow, CreateButton, CreateMessage, Message},
    async_trait,
};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};

use crate::{
    audio::{self, play_audio_track, AudioFile, RemoveAudioFile, TrackHandleHelper},
    common::{LogResult, UserData},
    config::Config,
    db::{self, AudioTable, AudioTableRowInsert, FtsCleanText},
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
    #[description = "Track to play"] audio_track: Option<String>,
) -> PoiseResult {
    let audio_track = audio_track.unwrap_or("wet-fart".into());
    log::info!("Playing audio track {audio_track}...");

    let guild_id = ctx.guild_id().ok_or("No guild id found")?;
    let channel_id = ctx.channel_id();
    let manager = ctx.songbird().await;

    let audio_track = audio_track.trim();

    // manager
    //     .play_audio(guild_id, channel_id, &audio_track)
    //     .await?;
    Ok(())
}

// #[poise::command(prefix_command, guild_only)]
// async fn list(ctx: PoiseContext<'_>, msg: &Message) -> PoiseResult {
//     let audio_tracks_md = audio::list_audio_track_names_markdown();

//     helpers::check_msg(msg.reply(ctx, audio_tracks_md).await);

//     Ok(())
// }

#[poise::command(prefix_command, guild_only)]
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
