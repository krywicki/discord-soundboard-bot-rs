use poise::{CreateReply, Modal};
use serenity::{all::CreateMessage, async_trait};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent};

use crate::{
    audio,
    common::{LogResult, UserData},
    db::{self, audio_table::AudioTableRowInsertBuilder, Tags},
    helpers::{self, check_msg, poise_check_msg, PoiseContextHelper, SongbirdHelper},
    vars,
};

pub type GenericError = Box<dyn std::error::Error + Send + Sync>;
pub type PoiseError = GenericError;
pub type PoiseContext<'a> = poise::Context<'a, UserData, PoiseError>;
pub type PoiseResult = Result<(), PoiseError>;
pub type PoiseAppContext<'a> = poise::ApplicationContext<'a, UserData, PoiseError>;

#[poise::command(prefix_command, guild_only)]
pub async fn deafen(_ctx: PoiseContext<'_>) -> PoiseResult {
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

    if let Ok(settings) = ctx.data().settings_table().get_settings().log_err() {
        if let Some(ref join_audio) = settings.join_audio {
            log::info!("Detected join audio: {join_audio}. Attempting to play.");
            match ctx
                .data()
                .audio_table()
                .find_audio_row(db::UniqueAudioTableCol::Name(join_audio.clone()))
            {
                Some(row) => {
                    log::debug!("bot join audio playing: {}", row.name);
                    manager
                        .play_audio(guild_id, connect_to, &row.audio_file)
                        .await
                        .log_err()
                        .ok();
                }
                None => log::error!("Couldn't locate join audio"),
            }
        }
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
        Some(_handler) => {
            // if leave audio set, play exit audio track
            if let Ok(settings) = ctx.data().settings_table().get_settings().log_err() {
                if let Some(ref leave_audio) = settings.leave_audio {
                    log::info!("Detected leave audio: {leave_audio}. Attempting to play.");
                    match ctx
                        .data()
                        .audio_table()
                        .find_audio_row(db::UniqueAudioTableCol::Name(leave_audio.clone()))
                    {
                        Some(row) => {
                            log::debug!("bot leave audio playing: {}", row.name);
                            manager
                                .play_audio_to_end(guild_id, channel_id, &row.audio_file)
                                .await
                                .log_err()
                                .ok();
                        }
                        None => log::error!("Couldn't locate leave audio"),
                    }
                }
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
            poise_check_msg(
                ctx.reply(format!("Playing track `{audio_track_name}`"))
                    .await,
            );

            manager
                .play_audio(guild_id, channel_id, &row.audio_file)
                .await?;
            table.increment_play_count(row.id).log_err()?;
        }
        None => poise_check_msg(
            ctx.reply(format!("Audio Track '{audio_track_name}' not found"))
                .await,
        ),
    }

    Ok(())
}

#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    subcommands(
        "add_sound",
        "remove_sound",
        "display_sounds",
        "edit_sound",
        "set_join_audio",
        "set_leave_audio",
        "display_help",
        "pin_sound",
        "unpin_sound"
    )
)]
pub async fn sounds(_ctx: PoiseContext<'_>) -> PoiseResult {
    log::warn!("/sounds command shouldn't be invoked direclty. It should just house sub commands");
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
    #[min_length = 3] // No length restriction by default (so, 1-4000 chars)
    #[max_length = 80] // Same as max button label len (crate::vars::BTN_LABEL_MAX_LEN)
    name: String,

    #[name = "Tags"] // Field name by default
    #[placeholder = "star Wars jedi luke skywalker force episode iv"] // No placeholder by default
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
            return Err("Can't add sound. It already exists".into()).log_err();
        }
        None => {
            let temp_audio_file = audio::download_audio_url_temp(&data.url).await?;

            // validate audio track (codec type, length, etc)
            audio::AudioFileValidator::default()
                .max_audio_duration(ctx.data().config.max_audio_file_duration)
                .reject_uuid_files(false)
                .validate(&temp_audio_file)?;

            // move track to sounds dir
            let audio_file = ctx.data().move_file_to_audio_dir(&temp_audio_file)?;
            let tags: Tags = match data.tags {
                Some(val) => Tags::from(val),
                None => Tags::new(),
            };

            table
                .insert_audio_row(
                    AudioTableRowInsertBuilder::new(data.name.clone(), audio_file)
                        .author_global_name(ctx.author().global_name.clone())
                        .author_id(Some(ctx.author().id.into()))
                        .author_name(Some(ctx.author().name.clone()))
                        .tags(tags)
                        .build(),
                )
                .log_err()?;
        }
    }

    poise_check_msg(
        ctx.reply(format!("Added sound `{}` to soundboard", data.name))
            .await,
    );

    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "remove")]
pub async fn remove_sound(
    ctx: PoiseContext<'_>,
    #[rename = "track"]
    #[description = "Track to play"]
    #[autocomplete = "helpers::autocomplete_audio_track_name"]
    audio_track_name: String,
) -> PoiseResult {
    log::info!("Removing audio track - {audio_track_name}");
    let table = ctx.data().audio_table();

    table.delete_audio_row(db::UniqueAudioTableCol::Name(audio_track_name.clone()))?;
    poise_check_msg(
        ctx.reply(format!("Removed audio track `{audio_track_name}`"))
            .await,
    );

    log::info!("Audio track removed {audio_track_name}");
    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "pin")]
pub async fn pin_sound(
    ctx: PoiseContext<'_>,
    #[rename = "track"]
    #[description = "Audio track to pin"]
    #[autocomplete = "helpers::autocomplete_audio_track_name"]
    audio_track_name: String,
) -> PoiseResult {
    log::info!("Pinning audio track - {audio_track_name}");

    let table = ctx.data().audio_table();
    table
        .update_audio_row_pin_by_name(&audio_track_name, true)
        .log_err()?;

    ctx.reply(format!("Pinned audio track `{audio_track_name}`"))
        .await?;

    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "unpin")]
pub async fn unpin_sound(
    ctx: PoiseContext<'_>,
    #[rename = "track"]
    #[description = "Audio track to pin"]
    #[autocomplete = "helpers::autocomplete_audio_track_name"]
    audio_track_name: String,
) -> PoiseResult {
    log::info!("Unpinning audio track - {audio_track_name}");

    let table = ctx.data().audio_table();
    table
        .update_audio_row_pin_by_name(&audio_track_name, false)
        .log_err()?;

    ctx.reply(format!("Unpinned audio track `{audio_track_name}`"))
        .await?;

    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "display")]
pub async fn display_sounds(
    ctx: PoiseContext<'_>,
    #[description = "Filter displayed sounds by names & tags"] search: Option<String>,
) -> PoiseResult {
    log::info!("`/sounds display` slash command received");

    match search.as_ref() {
        Some(value) => {
            poise_check_msg(
                ctx.reply(format!("Display searched sounds for `{value}`"))
                    .await,
            );

            let paginator = db::AudioTablePaginator::builder(ctx.data().db_connection())
                .fts_filter(search)
                .page_limit(vars::ACTION_ROWS_LIMIT)
                .build();

            for audio_rows in paginator {
                let audio_rows = audio_rows.log_err()?;

                // ActionRows: Have a 5x5 grid limit
                // (https://discordjs.guide/message-components/action-rows.html#action-rows)
                let btn_grid: Vec<_> = audio_rows.chunks(5).map(helpers::make_action_row).collect();
                let builder = CreateMessage::new().components(btn_grid);
                check_msg(ctx.channel_id().send_message(&ctx.http(), builder).await);
            }
        }
        None => {
            let reply = ctx.reply_builder(
                CreateReply::default()
                    .content("**Soundbot Controls**")
                    .components(helpers::make_soundbot_control_components()),
            );

            poise_check_msg(ctx.send(reply).await);
        }
    }

    Ok(())
}

#[derive(Debug, poise::Modal)]
#[name = "Edit Sound"]
struct EditSoundModal {
    #[name = "Name"]
    #[min_length = 3] // No length restriction by default (so, 1-4000 chars)
    #[max_length = 80] // Same as max button label len (crate::vars::BTN_LABEL_MAX_LEN)
    name: String,
    #[name = "Tags"]
    #[max_length = 1024]
    tags: Option<String>,
}

#[poise::command(slash_command, guild_only, rename = "edit")]
pub async fn edit_sound(
    ctx: PoiseAppContext<'_>,
    #[description = "Audio track to edit"]
    #[rename = "track"]
    #[autocomplete = "helpers::autocomplete_audio_track_name"]
    audio_track_name: String,
) -> PoiseResult {
    log::info!("Editing audio track - {audio_track_name}");

    let table = ctx.data().audio_table();

    let mut row = table
        .find_audio_row(db::UniqueAudioTableCol::Name(audio_track_name.clone()))
        .ok_or(format!("Unable to locate audio track '{audio_track_name}'"))
        .log_err()?;

    let data = EditSoundModal::execute_with_defaults(
        ctx,
        EditSoundModal {
            name: audio_track_name.clone(),
            tags: Some(row.tags.to_string()),
        },
    )
    .await?;

    match data {
        Some(data) => {
            log::debug!("{data:?}");
            let tags: Tags = match data.tags {
                Some(val) => Tags::from(val),
                None => Tags::new(),
            };

            row.name = data.name.clone();
            row.tags = tags;

            table.update_audio_row(&row).log_err()?;
        }
        None => log::info!("No audo track to update"),
    }

    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "join-audio")]
pub async fn set_join_audio(
    ctx: PoiseContext<'_>,
    #[description = "Audio track name"]
    #[rename = "track"]
    #[autocomplete = "helpers::autocomplete_opt_audio_track_name"]
    audio_track_name: String,
) -> PoiseResult {
    log::info!("Setting join audio: {audio_track_name:?}");

    let table = ctx.data().settings_table();
    let mut settings = table.get_settings().log_err()?;

    match audio_track_name.as_str() {
        "NONE" => {
            settings.join_audio = None;
            table.update_settings(&settings).log_err()?;
            poise_check_msg(ctx.reply(format!("Bot join audio disabled")).await);
        }
        val => {
            settings.join_audio = Some(val.into());
            table.update_settings(&settings).log_err()?;
            poise_check_msg(ctx.reply(format!("Bot join audio set to {val}")).await);
        }
    }
    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "leave-audio")]
pub async fn set_leave_audio(
    ctx: PoiseContext<'_>,
    #[description = "Audio track name"]
    #[rename = "track"]
    #[autocomplete = "helpers::autocomplete_opt_audio_track_name"]
    audio_track_name: String,
) -> PoiseResult {
    log::info!("Setting leave audio: {audio_track_name:?}");

    let table = ctx.data().settings_table();
    let mut settings = table.get_settings().log_err()?;

    match audio_track_name.as_str() {
        "NONE" => {
            settings.leave_audio = None;
            table.update_settings(&settings).log_err()?;
            poise_check_msg(ctx.reply(format!("Bot leave audio disabled")).await);
        }
        val => {
            settings.leave_audio = Some(val.into());
            table.update_settings(&settings).log_err()?;
            poise_check_msg(ctx.reply(format!("Bot leave audio set to {val}")).await);
        }
    }

    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "help")]
pub async fn display_help(ctx: PoiseContext<'_>) -> PoiseResult {
    let version = vars::VERSION;
    let prefix = ctx.data().config.command_prefix.as_str();
    let text = format!(
        "\
# Soundboard Bot v{version}
[GitHub Repo](https://github.com/krywicki/discord-soundboard-bot)
Bot for playing sounds in voice chat.
## Slash Commands
- `/play {{track}}` - Play sound track in voice channel
- `/sounds`
  - `/sounds add` - Opens form to add sounds
  - `/sounds remove {{track}}` - Removes sound
  - `/sounds edit {{track}}` - Opens form to edit sound track
  - `/sounds display` - Displays sound dispaly options
    - `/sounds display {{search}}` - Displays a button grid of sounds filtered by tags or names
  - `/sounds pin {{track}}` - Pin sound track to top of sound list
  - `/sounds unpin {{track}}` - Unpin sound track from top of sound list
  - `/sounds join-audio {{track}}` - Set/Unset sound track to play when bot joins voice channel
  - `/sounds leave-audio {{track}}` - Set/Unset sound track to play when bot leaves voice channel
## Prefix Commands
- `{prefix}join` - Have bot join the voice channel
- `{prefix}leave` - Have bot leave the voice channel
- `{prefix}register` - [`dev use`] Register/UnRegister slash commands for guild or globally
- `{prefix}scan` - [`dev use`] Scan local audio directory and add sound tracks not in database
"
    );

    poise_check_msg(ctx.reply(text).await);
    Ok(())
}

#[derive(Debug, poise::ChoiceParameter)]
pub enum Date {
    #[name = "Recent"]
    Date,
    #[name = "Latest"]
    DateReverse,
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
