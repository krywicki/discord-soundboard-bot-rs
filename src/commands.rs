use poise::command;
use serenity::{
    all::{CreateActionRow, CreateButton, CreateMessage, Message},
    async_trait,
};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};

use crate::{
    audio::{self, play_audio_track, TrackHandleHelper},
    config::Config,
    helpers::{self, check_msg, poise_check_msg, poise_songbird_get, SongbirdHelper},
    vars,
};

pub type GenericError = Box<dyn std::error::Error + Send + Sync>;
pub type PoiseError = GenericError;
pub type PoiseContext<'a> = poise::Context<'a, UserData, PoiseError>;
pub type PoiseResult = Result<(), PoiseError>;

pub struct UserData {
    pub config: Config,
}

impl UserData {
    pub fn builder() -> UserDataBuilder {
        UserDataBuilder(UserData::default())
    }
}

impl Default for UserData {
    fn default() -> Self {
        Self {
            config: Config::default(),
        }
    }
}

pub struct UserDataBuilder(UserData);

impl UserDataBuilder {
    pub fn config(mut self, value: Config) -> Self {
        self.0.config = value;
        self
    }

    pub fn build(self) -> UserData {
        self.0
    }
}

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
    let manager = poise_songbird_get(&ctx).await;

    let audio_track = audio_track.trim();

    manager
        .play_audio(guild_id, channel_id, &audio_track)
        .await?;
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
    let audio_tracks = audio::list_audio_track_names();

    let mut action_grids: Vec<Vec<CreateActionRow>> = vec![];

    for grid in audio_tracks.chunks(25) {
        // NOTE: ActionRows: Have a 5x5 grid limit
        //  (https://discordjs.guide/message-components/action-rows.html#action-rows)
        let mut action_rows = vec![];
        for audio_tracks_row in grid.chunks(5) {
            let mut buttons = vec![];
            for audio_track in audio_tracks_row {
                // create label (will be truncated if over 20 chars)
                let label = helpers::truncate_button_label(audio_track);

                let button =
                    CreateButton::new(helpers::ButtonCustomId::PlayAudio(audio_track.clone()))
                        .label(label);

                buttons.push(button);
            }

            action_rows.push(CreateActionRow::Buttons(buttons));
        }

        action_grids.push(action_rows);
    }

    for action_grid in action_grids {
        let builder = CreateMessage::new().components(action_grid);
        check_msg(ctx.channel_id().send_message(&ctx.http(), builder).await);
    }

    Ok(())
}

// #[poise::command(prefix_command, guild_only)]
// async fn help(ctx: PoiseContext<'_>, msg: &Message) -> PoiseResult {
//     Ok(())
// }

// #[poise::command(prefix_command, guild_only)]
// async fn scan(ctx: PoiseContext<'_>, msg: &Message) -> PoiseResult {
//     Ok(())
// }

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
