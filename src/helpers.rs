use core::fmt;
use std::num::ParseIntError;
use std::sync::Arc;

use poise::CreateReply;
use serenity::all::{
    ChannelId, CreateActionRow, CreateButton, CreateInteractionResponseMessage, CreateMessage,
    CreateSelectMenuOption, GuildId, ReactionType,
};
use serenity::async_trait;
use serenity::client::Context;
use songbird::tracks::TrackHandle;
use songbird::{Songbird, SongbirdKey};

use crate::audio::TrackHandleHelper;
use crate::commands::{PoiseContext, PoiseError, PoiseResult};
use crate::common::LogResult;
use crate::db::paginators::PaginateInfo;
use crate::db::AudioTableRow;
use crate::errors::AudioError;
use crate::vars;
use crate::{audio, db};

pub async fn songbird_get(ctx: &Context) -> Arc<songbird::Songbird> {
    songbird::get(ctx)
        .await
        .expect("Songbird voice client placed in at initialization")
        .clone()
}

pub async fn poise_songbird_get(ctx: &PoiseContext<'_>) -> Arc<songbird::Songbird> {
    let data = ctx.serenity_context().data.read().await;
    data.get::<SongbirdKey>()
        .expect("Songbird voice client placed in at initialization")
        .clone()
}

pub fn poise_check_msg(result: Result<poise::ReplyHandle, serenity::Error>) {
    if let Err(err) = result {
        log::error!("Error sending message: {:?}", err);
    }
}

pub async fn is_bot_alone_in_voice_channel(
    ctx: &Context,
    guild_id: GuildId,
) -> Result<bool, PoiseError> {
    if let Some(bot_voice_channel_id) = get_bot_voice_channel_id(ctx, guild_id).await {
        if let Some(guild) = ctx.cache.guild(guild_id) {
            if let Some(channel) = guild.channels.get(&bot_voice_channel_id) {
                let members = channel.members(&ctx)?;
                return Ok(members.len() == 1 && members[0].user.id == ctx.cache.current_user().id);
            }
        }
    }

    Ok(false)
}

pub async fn get_bot_voice_channel_id(ctx: &Context, guild_id: GuildId) -> Option<ChannelId> {
    let user = ctx.cache.current_user();
    let bot_id = user.id;

    // Get the guild from the cache
    let guild = ctx.cache.guild(guild_id)?;

    // Get the voice states for the guild
    let voice_state = guild.voice_states.get(&bot_id)?;

    voice_state.channel_id
}

#[derive(Debug, PartialEq)]
pub enum DisplayMenuItemCustomId {
    DisplayAll,
    DisplayPinned,
    DisplayMostPlayed,
    DisplayRecentlyAdded,
    Unknown(String),
}

impl DisplayMenuItemCustomId {
    pub const CUSTOM_ID: &'static str = "sound_bot_display_menu";
}

impl From<&String> for DisplayMenuItemCustomId {
    fn from(value: &String) -> Self {
        match value.as_str() {
            "sound_bot_display_menu_item_pinned" => Self::DisplayPinned,
            "sound_bot_display_menu_item_all" => Self::DisplayAll,
            "sound_bot_display_menu_item_most_played" => Self::DisplayMostPlayed,
            "sound_bot_display_menu_item_recently_added" => Self::DisplayRecentlyAdded,
            _ => Self::Unknown(value.clone()),
        }
    }
}

impl From<String> for DisplayMenuItemCustomId {
    fn from(value: String) -> Self {
        DisplayMenuItemCustomId::from(&value)
    }
}

impl From<DisplayMenuItemCustomId> for String {
    fn from(value: DisplayMenuItemCustomId) -> Self {
        match value {
            DisplayMenuItemCustomId::DisplayPinned => format!("sound_bot_display_menu_item_pinned"),
            DisplayMenuItemCustomId::DisplayAll => format!("sound_bot_display_menu_item_all"),
            DisplayMenuItemCustomId::DisplayMostPlayed => {
                format!("sound_bot_display_menu_item_most_played")
            }
            DisplayMenuItemCustomId::DisplayRecentlyAdded => {
                format!("sound_bot_display_menu_item_recently_added")
            }
            DisplayMenuItemCustomId::Unknown(val) => val,
        }
    }
}

#[derive(Debug)]
pub enum PaginateId {
    RecentlyAddedFirstPage(u64),
    RecentlyAddedLastPage(u64),
    RecentlyAddedNextPage(u64),
    RecentlyAddedPrevPage(u64),
    AllFirstPage(u64),
    AllLastPage(u64),
    AllNextPage(u64),
    AllPrevPage(u64),
    MostPlayedFirstPage(u64),
    MostPlayedLastPage(u64),
    MostPlayedNextPage(u64),
    MostPlayedPrevPage(u64),
    SearchFirstPage(u64, String),
    SearchLastPage(u64, String),
    SearchNextPage(u64, String),
    SearchPrevPage(u64, String),
    PinnedFirstPage(u64),
    PinnedLastPage(u64),
    PinnedNextPage(u64),
    PinnedPrevPage(u64),
    Unknown(String),
}

impl TryFrom<&String> for PaginateId {
    type Error = String;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        let parts: Vec<_> = value.split("::").collect();

        let parse_offset_fn = |val: &str| {
            val.parse()
                .map_err(|e: ParseIntError| e.to_string())
                .log_err_op(|e| format!("Parse error on button page offset value: '{value}' - {e}"))
        };

        match parts[0] {
            "recently_added_first_page" => Ok(PaginateId::RecentlyAddedFirstPage(parse_offset_fn(
                parts[1],
            )?)),
            "recently_added_last_page" => Ok(PaginateId::RecentlyAddedLastPage(parse_offset_fn(
                parts[1],
            )?)),
            "recently_added_next_page" => Ok(PaginateId::RecentlyAddedNextPage(parse_offset_fn(
                parts[1],
            )?)),
            "recently_added_prev_page" => Ok(PaginateId::RecentlyAddedPrevPage(parse_offset_fn(
                parts[1],
            )?)),
            "all_first_page" => Ok(PaginateId::AllFirstPage(parse_offset_fn(parts[1])?)),
            "all_last_page" => Ok(PaginateId::AllLastPage(parse_offset_fn(parts[1])?)),
            "all_next_page" => Ok(PaginateId::AllNextPage(parse_offset_fn(parts[1])?)),
            "all_prev_page" => Ok(PaginateId::AllPrevPage(parse_offset_fn(parts[1])?)),
            "most_played_first_page" => Ok(Self::MostPlayedFirstPage(parse_offset_fn(parts[1])?)),
            "most_played_last_page" => Ok(Self::MostPlayedLastPage(parse_offset_fn(parts[1])?)),
            "most_played_next_page" => {
                Ok(PaginateId::MostPlayedNextPage(parse_offset_fn(parts[1])?))
            }
            "most_played_prev_page" => {
                Ok(PaginateId::MostPlayedPrevPage(parse_offset_fn(parts[1])?))
            }
            "pinned_first_page" => Ok(PaginateId::PinnedFirstPage(parse_offset_fn(parts[1])?)),
            "pinned_last_page" => Ok(PaginateId::PinnedLastPage(parse_offset_fn(parts[1])?)),
            "pinned_next_page" => Ok(PaginateId::PinnedNextPage(parse_offset_fn(parts[1])?)),
            "pinned_prev_page" => Ok(PaginateId::PinnedPrevPage(parse_offset_fn(parts[1])?)),
            "search_first_page" => Ok(PaginateId::SearchFirstPage(
                parse_offset_fn(parts[1])?,
                parts[2..].join("").into(),
            )),
            "search_last_page" => Ok(PaginateId::SearchFirstPage(
                parse_offset_fn(parts[1])?,
                parts[2..].join("").into(),
            )),
            "search_next_page" => Ok(PaginateId::SearchNextPage(
                parse_offset_fn(parts[1])?,
                parts[2..].join("").into(),
            )),
            "search_prev_page" => Ok(PaginateId::SearchPrevPage(
                parse_offset_fn(parts[1])?,
                parts[2..].join("").into(),
            )),
            val => Ok(Self::Unknown(val.into())),
        }
    }
}

impl TryFrom<String> for PaginateId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        PaginateId::try_from(&value)
    }
}

impl From<&PaginateId> for String {
    fn from(value: &PaginateId) -> Self {
        match value {
            PaginateId::AllFirstPage(val) => format!("all_first_page::{val}"),
            PaginateId::AllLastPage(val) => format!("all_last_page::{val}"),
            PaginateId::AllNextPage(val) => format!("all_next_page::{val}"),
            PaginateId::AllPrevPage(val) => format!("all_prev_page::{val}"),
            PaginateId::MostPlayedFirstPage(val) => format!("most_played_first_page::{val}"),
            PaginateId::MostPlayedLastPage(val) => format!("most_played_last_page::{val}"),
            PaginateId::MostPlayedNextPage(val) => {
                format!("most_played_next_page::{val}")
            }
            PaginateId::MostPlayedPrevPage(val) => {
                format!("most_played_prev_page::{val}")
            }
            PaginateId::RecentlyAddedFirstPage(val) => format!("recently_added_first_page::{val}"),
            PaginateId::RecentlyAddedLastPage(val) => format!("recently_added_last_page::{val}"),
            PaginateId::RecentlyAddedNextPage(val) => {
                format!("recently_added_next_page::{val}")
            }
            PaginateId::RecentlyAddedPrevPage(val) => {
                format!("recently_added_prev_page::{val}")
            }
            PaginateId::PinnedFirstPage(val) => format!("pinned_first_page::{val}"),
            PaginateId::PinnedLastPage(val) => format!("pinned_last_page::{val}"),
            PaginateId::PinnedNextPage(val) => format!("pinned_next_page::{val}"),
            PaginateId::PinnedPrevPage(val) => format!("pinned_prev_page::{val}"),
            PaginateId::SearchFirstPage(val, search) => {
                format!("search_first_page::{val}::{search}")
            }
            PaginateId::SearchLastPage(val, search) => format!("search_last_page::{val}::{search}"),
            PaginateId::SearchNextPage(val, search) => {
                format!("search_next_page::{val}::{search}")
            }
            PaginateId::SearchPrevPage(val, search) => {
                format!("search_prev_page::{val}::{search}")
            }

            PaginateId::Unknown(val) => val.clone(),
        }
    }
}

impl From<PaginateId> for String {
    fn from(value: PaginateId) -> Self {
        String::from(&value)
    }
}

impl fmt::Display for PaginateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = String::from(self);
        write!(f, "{s}")
    }
}

#[derive(Debug)]
pub enum ButtonCustomId {
    PlayAudio(i64),
    PlayRandom,
    Search,
    Paginate(PaginateId),
    Unknown(String),
}

impl TryFrom<&String> for ButtonCustomId {
    type Error = String;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        let parts: Vec<_> = value.split("::").collect();

        match parts[0] {
            "sound_bot_play" => {
                let id: i64 = parts[1]
                    .parse()
                    .map_err(|e: ParseIntError| e.to_string())
                    .log_err_op(|e| format!("Parse error on button custom id '{value}' - {e}"))?;
                Ok(ButtonCustomId::PlayAudio(id))
            }
            "sound_bot_play_random" => Ok(ButtonCustomId::PlayRandom),
            "sound_bot_search" => Ok(ButtonCustomId::Search),
            "sound_bot_paginate" => Ok(ButtonCustomId::Paginate(PaginateId::try_from(
                parts[1..].join("::").to_string(),
            )?)),
            _ => Ok(ButtonCustomId::Unknown(value.clone())),
        }
    }
}

impl TryFrom<String> for ButtonCustomId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        ButtonCustomId::try_from(&value)
    }
}

impl From<ButtonCustomId> for String {
    fn from(value: ButtonCustomId) -> Self {
        match value {
            ButtonCustomId::PlayAudio(val) => format!("sound_bot_play::{val}"),
            ButtonCustomId::PlayRandom => format!("sound_bot_play_random"),
            ButtonCustomId::Search => format!("sound_bot_search"),
            ButtonCustomId::Paginate(val) => format!("sound_bot_paginate::{val}"),
            ButtonCustomId::Unknown(val) => val,
        }
    }
}

pub trait ButtonLabel {
    fn to_button_label(&self) -> String;
}

impl ButtonLabel for String {
    fn to_button_label(&self) -> String {
        truncate_button_label(&self)
    }
}

impl ButtonLabel for &str {
    fn to_button_label(&self) -> String {
        truncate_button_label(&self)
    }
}

pub fn truncate_button_label(label: impl AsRef<str>) -> String {
    let label = label.as_ref();
    if label.len() > vars::BTN_LABEL_MAX_LEN {
        format!("{}...", label[0..(vars::BTN_LABEL_MAX_LEN - 3)].to_string())
    } else {
        label.to_string()
    }
}

/// Get voice channel the author of command is currently in.
/// Returns tuple (guild_id, channel_id)
pub fn get_author_voice_channel(ctx: &PoiseContext) -> Result<(GuildId, ChannelId), PoiseError> {
    match ctx.guild() {
        Some(guild) => {
            let channel_id = guild
                .voice_states
                .get(&ctx.author().id)
                .and_then(|voice_state| voice_state.channel_id);

            match channel_id {
                Some(channel_id) => Ok((guild.id, channel_id)),
                None => Err(
                    "Unable to get author voice channel. Missing voice states channel id.".into(),
                ),
            }
        }
        None => Err("Unable to get author voice channel. Missing ctx.guild()".into()),
    }
}

#[async_trait]
pub trait SongbirdHelper {
    /// Begins play audio track and returns handle to track
    async fn play_audio(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError>;

    /// Plays audio track all the way to the end, then returns audio track
    async fn play_audio_to_end(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError>;

    async fn leave_voice_channel(&self, guild_id: GuildId) -> PoiseResult;
}

#[async_trait]
impl SongbirdHelper for Songbird {
    async fn leave_voice_channel(&self, guild_id: GuildId) -> PoiseResult {
        log::info!("Songbird leaving voice channel for guild_id: {guild_id}");

        match self.get(guild_id) {
            Some(_handler) => {
                self.leave(guild_id).await.log_err()?;
            }
            None => {
                log::error!("Songbird manager does not have a handler for guild_id: {guild_id}")
            }
        }

        Ok(())
    }

    async fn play_audio(
        &self,
        guild_id: GuildId,
        _channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError> {
        log::debug!("Starting to play_audio_track - {audio_track:?}");

        let audio_input = songbird::input::File::new(audio_track.as_path_buf());

        match self.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;

                let track_handle = handler.play_input(audio_input.into());
                log::info!("Playing track {audio_track:?}");
                Ok(track_handle)
            }
            None => Err(AudioError::NotInVoiceChannel),
        }
    }

    async fn play_audio_to_end(
        &self,
        guild_id: GuildId,
        _channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError> {
        log::debug!("Starting to play_audio_track - {audio_track:?}");

        let audio_input = songbird::input::File::new(audio_track.as_path_buf());

        match self.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;

                let track_handle = handler.play_input(audio_input.into());
                log::info!("Playing track {audio_track:?}");

                track_handle.wait_for_end().await;
                Ok(track_handle)
            }
            None => Err(AudioError::NotInVoiceChannel),
        }
    }
}

#[async_trait]
pub trait PoiseContextHelper<'a> {
    async fn songbird(&self) -> Arc<songbird::Songbird>;
}

#[async_trait]
impl<'a> PoiseContextHelper<'a> for PoiseContext<'a> {
    async fn songbird(&self) -> Arc<songbird::Songbird> {
        let data = self.serenity_context().data.read().await;
        data.get::<SongbirdKey>()
            .expect("Songbird voice client placed in at initialization")
            .clone()
    }
}

pub fn make_action_row(audio_rows: &[AudioTableRow]) -> CreateActionRow {
    let buttons: Vec<_> = audio_rows
        .iter()
        .map(|track| {
            let style = if track.pinned {
                serenity::all::ButtonStyle::Success
            } else {
                serenity::all::ButtonStyle::Primary
            };

            CreateButton::new(ButtonCustomId::PlayAudio(track.id))
                .label(track.name.to_button_label())
                .style(style)
        })
        .collect();

    CreateActionRow::Buttons(buttons)
}

pub struct SoundDisplayMessage {
    content: String,
    components: Vec<CreateActionRow>,
}

impl SoundDisplayMessage {
    pub fn new(content: String, compnents: Vec<CreateActionRow>) -> Self {
        Self {
            content: content,
            components: compnents,
        }
    }
}

impl Into<CreateInteractionResponseMessage> for SoundDisplayMessage {
    fn into(self) -> CreateInteractionResponseMessage {
        CreateInteractionResponseMessage::new()
            .content(self.content)
            .components(self.components)
    }
}

impl Into<CreateMessage> for SoundDisplayMessage {
    fn into(self) -> CreateMessage {
        CreateMessage::new()
            .content(self.content)
            .components(self.components)
    }
}

impl Into<CreateReply> for SoundDisplayMessage {
    fn into(self) -> CreateReply {
        CreateReply::default()
            .content(self.content)
            .components(self.components)
    }
}

pub fn make_display_message(
    paginator: &mut db::AudioTablePaginator,
    display_type: DisplayType,
    search: Option<String>,
) -> Result<SoundDisplayMessage, String> {
    let paginate_info: PaginateInfo = paginator.pageinate_info()?;

    let title = make_display_title(display_type, &paginate_info, search.clone());
    let btn_grid: Vec<_> = paginator
        .next_page()?
        .chunks(5)
        .map(make_action_row)
        .collect();
    let paginate_ctrls = make_paginate_controls(display_type, &paginate_info, search.clone());

    // let sound_ctrls = if search.is_none() {
    //     make_soundbot_control_components(Some(display_type.into()))
    // } else {
    //     make_soundbot_control_components(None)
    // };

    let mut components: Vec<_> = vec![];
    components.extend(btn_grid);
    components.push(paginate_ctrls);
    //components.extend(sound_ctrls);

    Ok(SoundDisplayMessage::new(title, components))
}

pub fn make_sound_controls_message() -> SoundDisplayMessage {
    SoundDisplayMessage::new(
        "**Soundbot Controls**".into(),
        make_soundbot_control_components(None),
    )
}

pub fn make_soundbot_control_components(
    default_selected_menu_item: Option<DisplayMenuItemCustomId>,
) -> Vec<CreateActionRow> {
    vec![
        CreateActionRow::SelectMenu(
            serenity::builder::CreateSelectMenu::new(
                DisplayMenuItemCustomId::CUSTOM_ID,
                serenity::builder::CreateSelectMenuKind::String {
                    options: vec![
                        CreateSelectMenuOption::new(
                            "All Sounds",
                            DisplayMenuItemCustomId::DisplayAll,
                        )
                        .emoji(ReactionType::Unicode("📋".into()))
                        .default_selection(
                            default_selected_menu_item == Some(DisplayMenuItemCustomId::DisplayAll),
                        ),
                        CreateSelectMenuOption::new(
                            "Pinned Sounds",
                            DisplayMenuItemCustomId::DisplayPinned,
                        )
                        .emoji(ReactionType::Unicode("📋".into()))
                        .default_selection(
                            default_selected_menu_item
                                == Some(DisplayMenuItemCustomId::DisplayPinned),
                        ),
                        CreateSelectMenuOption::new(
                            "Recently Added Sounds",
                            DisplayMenuItemCustomId::DisplayRecentlyAdded,
                        )
                        .emoji(ReactionType::Unicode("📋".into()))
                        .default_selection(
                            default_selected_menu_item
                                == Some(DisplayMenuItemCustomId::DisplayRecentlyAdded),
                        ),
                        CreateSelectMenuOption::new(
                            "Most Played Sounds",
                            DisplayMenuItemCustomId::DisplayMostPlayed,
                        )
                        .emoji(ReactionType::Unicode("📋".into()))
                        .default_selection(
                            default_selected_menu_item
                                == Some(DisplayMenuItemCustomId::DisplayMostPlayed),
                        ),
                    ],
                },
            )
            .placeholder("Display Sounds"),
        ),
        CreateActionRow::Buttons(vec![
            CreateButton::new(ButtonCustomId::Search)
                .label("Search".to_string())
                .emoji(ReactionType::Unicode("🔍".into()))
                .style(serenity::all::ButtonStyle::Secondary),
            CreateButton::new(ButtonCustomId::PlayRandom)
                .label("Play Random".to_string())
                .emoji(ReactionType::Unicode("🎵".into()))
                .style(serenity::all::ButtonStyle::Secondary),
        ]),
    ]
}

pub async fn autocomplete_audio_track_name<'a>(
    ctx: PoiseContext<'_>,
    partial: &'a str,
) -> impl futures::stream::Stream<Item = String> + 'a {
    let table = ctx.data().audio_table();
    let track_names = table.fts_autocomplete_track_names(partial, Some(5));
    futures::stream::iter(track_names)
}

pub async fn autocomplete_opt_audio_track_name<'a>(
    ctx: PoiseContext<'_>,
    partial: &'a str,
) -> impl futures::stream::Stream<Item = String> + 'a {
    let table = ctx.data().audio_table();
    let mut track_names = table.fts_autocomplete_track_names(partial, Some(5));
    track_names.insert(0, "NONE".into());

    futures::stream::iter(track_names)
}

pub fn uuid_v4_str() -> String {
    // Create uuid audio file in /tmp directory
    let uuid = uuid::Uuid::new_v4();
    let mut encode_buf = uuid::Uuid::encode_buffer();
    uuid.hyphenated().encode_lower(&mut encode_buf).to_string()
}

#[derive(Debug, Copy, Clone)]
pub enum DisplayType {
    All,
    RecentlyAdded,
    MostPlayed,
    Pinned,
    Search,
}

impl From<DisplayType> for DisplayMenuItemCustomId {
    fn from(value: DisplayType) -> Self {
        match value {
            DisplayType::All => DisplayMenuItemCustomId::DisplayAll,
            DisplayType::MostPlayed => DisplayMenuItemCustomId::DisplayMostPlayed,
            DisplayType::RecentlyAdded => DisplayMenuItemCustomId::DisplayRecentlyAdded,
            DisplayType::Pinned => DisplayMenuItemCustomId::DisplayPinned,
            DisplayType::Search => DisplayMenuItemCustomId::Unknown("".into()),
        }
    }
}

impl From<DisplayMenuItemCustomId> for DisplayType {
    fn from(value: DisplayMenuItemCustomId) -> Self {
        match value {
            DisplayMenuItemCustomId::DisplayAll => Self::All,
            DisplayMenuItemCustomId::DisplayMostPlayed => Self::MostPlayed,
            DisplayMenuItemCustomId::DisplayRecentlyAdded => Self::RecentlyAdded,
            DisplayMenuItemCustomId::DisplayPinned => Self::Pinned,
            DisplayMenuItemCustomId::Unknown(_) => Self::All,
        }
    }
}

pub fn make_paginate_controls(
    display_type: DisplayType,
    paginate_info: &PaginateInfo,
    search: Option<String>,
) -> CreateActionRow {
    let (first_btn, prev_btn, next_btn, last_btn) = match display_type {
        DisplayType::All => {
            let first_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::AllFirstPage(
                paginate_info.first_page_offset.unwrap_or(0),
            )))
            .disabled(paginate_info.first_page_offset.is_none());

            let last_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::AllLastPage(
                paginate_info.last_page_offset.unwrap_or(0),
            )))
            .disabled(paginate_info.last_page_offset.is_none());

            let prev_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::AllPrevPage(
                paginate_info.prev_page_offset.unwrap_or(0),
            )))
            .disabled(paginate_info.prev_page_offset.is_none());

            let next_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::AllNextPage(
                paginate_info.next_page_offset.unwrap_or(0),
            )))
            .disabled(paginate_info.next_page_offset.is_none());

            (first_btn, prev_btn, next_btn, last_btn)
        }
        DisplayType::MostPlayed => {
            let first_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::MostPlayedFirstPage(paginate_info.first_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.first_page_offset.is_none());

            let last_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::MostPlayedLastPage(paginate_info.last_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.last_page_offset.is_none());

            let prev_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::MostPlayedPrevPage(paginate_info.prev_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.prev_page_offset.is_none());

            let next_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::MostPlayedNextPage(paginate_info.next_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.next_page_offset.is_none());

            (first_btn, prev_btn, next_btn, last_btn)
        }
        DisplayType::RecentlyAdded => {
            let first_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::RecentlyAddedFirstPage(paginate_info.first_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.first_page_offset.is_none());

            let last_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::RecentlyAddedLastPage(paginate_info.last_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.last_page_offset.is_none());

            let prev_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::RecentlyAddedPrevPage(paginate_info.prev_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.prev_page_offset.is_none());

            let next_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::RecentlyAddedNextPage(paginate_info.next_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.next_page_offset.is_none());

            (first_btn, prev_btn, next_btn, last_btn)
        }
        DisplayType::Pinned => {
            let first_btn = CreateButton::new(ButtonCustomId::Paginate(
                PaginateId::PinnedFirstPage(paginate_info.first_page_offset.unwrap_or(0)),
            ))
            .disabled(paginate_info.first_page_offset.is_none());

            let last_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::PinnedLastPage(
                paginate_info.last_page_offset.unwrap_or(0),
            )))
            .disabled(paginate_info.last_page_offset.is_none());

            let prev_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::PinnedPrevPage(
                paginate_info.prev_page_offset.unwrap_or(0),
            )))
            .disabled(paginate_info.prev_page_offset.is_none());

            let next_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::PinnedNextPage(
                paginate_info.next_page_offset.unwrap_or(0),
            )))
            .disabled(paginate_info.next_page_offset.is_none());

            (first_btn, prev_btn, next_btn, last_btn)
        }
        DisplayType::Search => {
            let search = search.unwrap_or("".into());

            let first_btn =
                CreateButton::new(ButtonCustomId::Paginate(PaginateId::SearchFirstPage(
                    paginate_info.first_page_offset.unwrap_or(0),
                    search.clone(),
                )))
                .disabled(paginate_info.first_page_offset.is_none());

            let last_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::SearchLastPage(
                paginate_info.last_page_offset.unwrap_or(0),
                search.clone(),
            )))
            .disabled(paginate_info.last_page_offset.is_none());

            let prev_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::SearchPrevPage(
                paginate_info.prev_page_offset.unwrap_or(0),
                search.clone(),
            )))
            .disabled(paginate_info.prev_page_offset.is_none());

            let next_btn = CreateButton::new(ButtonCustomId::Paginate(PaginateId::SearchNextPage(
                paginate_info.next_page_offset.unwrap_or(0),
                search,
            )))
            .disabled(paginate_info.next_page_offset.is_none());

            (first_btn, prev_btn, next_btn, last_btn)
        }
    };

    let first_btn = first_btn
        .style(serenity::all::ButtonStyle::Secondary)
        .emoji(ReactionType::Unicode("⏮️".into()));
    let prev_btn = prev_btn
        .style(serenity::all::ButtonStyle::Secondary)
        .emoji(ReactionType::Unicode("◀️".into()));
    let next_btn = next_btn
        .style(serenity::all::ButtonStyle::Secondary)
        .emoji(ReactionType::Unicode("▶️".into()));
    let last_btn = last_btn
        .style(serenity::all::ButtonStyle::Secondary)
        .emoji(ReactionType::Unicode("⏭️".into()));

    CreateActionRow::Buttons(vec![first_btn, prev_btn, next_btn, last_btn])
}

pub fn make_display_title(
    display_type: DisplayType,
    paginate_info: &PaginateInfo,
    search: Option<String>,
) -> String {
    let cur_page = paginate_info.cur_page;
    let total_pages = paginate_info.total_pages;

    match display_type {
        DisplayType::All => format!("### All Sounds (page {cur_page} of {total_pages})..."),
        DisplayType::MostPlayed => {
            format!("### Most Played Sounds (page {cur_page} of {total_pages})...")
        }
        DisplayType::RecentlyAdded => {
            format!("### Recently Added Sounds (page {cur_page} of {total_pages})...")
        }
        DisplayType::Search => {
            format!(
                "### Search Results `{}` (page {cur_page} of {total_pages})...",
                search.unwrap_or(String::new())
            )
        }
        DisplayType::Pinned => {
            format!("### Pinned Sounds (page {cur_page} of {total_pages})...")
        }
    }
}
