use serenity::all::{
    Attachment, CacheHttp, ComponentInteraction, ComponentInteractionDataKind, Context,
    CreateActionRow, CreateButton, CreateInteractionResponse, CreateInteractionResponseMessage,
    CreateMessage, CreateQuickModal, FullEvent, Interaction, Message, VoiceState,
};

use crate::{
    commands::PoiseResult,
    common::{LogResult, UserData},
    db::{self, AudioTable, SettingsTable, Table, Tags},
    helpers::{self, ButtonCustomId, DisplayMenuItemCustomId, PaginateId, SongbirdHelper},
    FrameworkContext,
};

pub async fn event_handler(
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
        FullEvent::Message { new_message } => {
            handle_message(ctx, framework, data, new_message).await?
        }
        _ => {}
    }

    Ok(())
}

pub async fn handle_ready(
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

pub async fn handle_message(
    _ctx: &Context,
    _framework: FrameworkContext<'_>,
    data: &UserData,
    new_message: &Message,
) -> PoiseResult {
    // handle mp3 file

    if let Some(attachment) = new_message.attachments.first() {
        const DEFAULT_STR: String = String::new();
        match attachment
            .content_type
            .as_ref()
            .unwrap_or(&DEFAULT_STR)
            .as_str()
        {
            "audio/mpeg" | "audio/mpeg3" | "x-mpeg-3" => {
                if (attachment.size as u64) < crate::audio::MAX_AUDIO_FILE_LENGTH_BYTES {
                    handle_attached_mp3_message(_ctx, _framework, data, new_message, &attachment)
                        .await?
                }
            }
            _ => {}
        }
    }

    Ok(())
}

pub async fn handle_attached_mp3_message(
    ctx: &Context,
    _framework: FrameworkContext<'_>,
    _data: &UserData,
    new_message: &Message,
    mp3_attachment: &Attachment,
) -> PoiseResult {
    log::info!("handle mp3 attached file");

    let msg = CreateMessage::new()
        .content(format!(
            "Do you want to add `{}` to soundbot?",
            mp3_attachment.filename
        ))
        .components(vec![CreateActionRow::Buttons(vec![
            CreateButton::new(ButtonCustomId::AddMp3File)
                .label("Add To Soundbot")
                .style(serenity::all::ButtonStyle::Secondary)
                .emoji(serenity::all::ReactionType::Unicode("🎵".into())),
            CreateButton::new(ButtonCustomId::IgnoreMp3File)
                .label("Ignore")
                .style(serenity::all::ButtonStyle::Secondary)
                .emoji(serenity::all::ReactionType::Unicode("🛑".into())),
        ])])
        .reference_message(new_message);

    new_message
        .channel_id
        .send_message(&ctx.http(), msg)
        .await
        .log_err_msg("Failed sending handle attached mp3 reply")?;

    Ok(())
}

pub async fn handle_voice_state_update(
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

pub async fn handle_interaction_create(
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

pub async fn handle_component_interaction(
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
        ComponentInteractionDataKind::StringSelect { ref values } => {
            handle_string_select_interaction(ctx, interaction, component, framework, data, &values)
                .await?
        }
        _ => {}
    }

    Ok(())
}

pub async fn handle_btn_interaction(
    ctx: &Context,
    interaction: &Interaction,
    component: &ComponentInteraction,
    framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::debug!("Interaction Component Button pressed");
    let custom_id = &component.data.custom_id;

    match custom_id.as_str() {
        "soundbot_add_mp3_file" => {
            log::info!("soundbot_add_mp3_file===> {:?}", component.message);
        }
        "soundbot_ignore_mp3_file" => {
            log::info!("soundbot_ignore_mp3_file===> {:?}", component.message);
        }
        _ => {}
    }

    let button_id = ButtonCustomId::try_from(custom_id)?;
    match button_id {
        ButtonCustomId::PlayAudio(audio_track_id) => {
            handle_play_audio_btn(ctx, interaction, component, framework, data, audio_track_id)
                .await?;
        }
        ButtonCustomId::PlayRandom => {
            handle_play_random_btn(ctx, interaction, component, framework, data).await?;
        }
        ButtonCustomId::Search => {
            handle_search_btn(ctx, interaction, component, framework, data).await?;
        }

        ButtonCustomId::Paginate(val) => {
            handle_paginate_btn(ctx, interaction, component, framework, data, val).await?;
        }
        ButtonCustomId::AddMp3File => {
            handle_add_mp3_file_btn(ctx, interaction, component, framework, data).await?;
        }
        ButtonCustomId::IgnoreMp3File => {
            handle_ignore_mp3_file_btn(ctx, interaction, component, framework, data).await?;
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

pub async fn handle_string_select_interaction(
    ctx: &Context,
    interaction: &Interaction,
    component: &ComponentInteraction,
    framework: FrameworkContext<'_>,
    data: &UserData,
    values: &Vec<String>,
) -> PoiseResult {
    log::debug!("Interaction Component string select");
    let custom_id = &component.data.custom_id;

    match custom_id.as_str() {
        DisplayMenuItemCustomId::CUSTOM_ID => {
            handle_display_select_menu(ctx, interaction, component, framework, data, &values)
                .await?;
        }
        val => {
            component
                .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
                .await
                .log_err_msg("Failed to create response for btn interaction")
                .ok();
            log::warn!("string select interaction custom_id({val}) not handled");
        }
    }

    Ok(())
}

pub async fn handle_display_select_menu(
    ctx: &Context,
    interaction: &Interaction,
    component: &ComponentInteraction,
    framework: FrameworkContext<'_>,
    data: &UserData,
    values: &Vec<String>,
) -> PoiseResult {
    log::info!("display select menu values: {:?}", values);

    let menu_item_id = values
        .get(0)
        .ok_or("no menu item id")
        .log_err_msg("handle display select menu err")?;

    match DisplayMenuItemCustomId::from(menu_item_id) {
        DisplayMenuItemCustomId::DisplayAll => {
            handle_display_all_menu_select(ctx, interaction, component, framework, data).await?;
        }
        DisplayMenuItemCustomId::DisplayPinned => {
            handle_display_pinned_menu_select(ctx, interaction, component, framework, data).await?;
        }
        DisplayMenuItemCustomId::DisplayMostPlayed => {
            handle_display_most_played_menu_select(ctx, interaction, component, framework, data)
                .await?;
        }
        DisplayMenuItemCustomId::DisplayRecentlyAdded => {
            handle_display_recently_added_menu_select(ctx, interaction, component, framework, data)
                .await?;
        }
        DisplayMenuItemCustomId::Unknown(value) => {
            return Err(format!(
                "Unrecognized button custom_id({value}) for component interaction."
            )
            .into())
            .log_err();
        }
    }

    Ok(())
}

pub async fn handle_add_mp3_file_btn(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!("Handle add MP3 file button");

    let send_ref_msg_404_fn = async |msg: String| {
        component
            .create_response(
                &ctx.http(),
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .content(msg)
                        .components(vec![]),
                ),
            )
            .await
            .log_err()
    };

    let channel_id = component.channel_id;
    let ref_message = if let Some(message_ref) = component.message.message_reference.as_ref() {
        if let Some(message_id) = message_ref.message_id {
            match channel_id.message(&ctx.http(), message_id).await {
                Ok(message) => message,
                Err(err) => {
                    log::error!("{err}");
                    send_ref_msg_404_fn(
                        "Failed to locate referenced message with attached MP3 file".into(),
                    )
                    .await
                    .log_err()?;

                    return Ok(());
                }
            }
        } else {
            send_ref_msg_404_fn(
                "Failed to locate referenced message with attached MP3 file".into(),
            )
            .await
            .log_err()
            .ok();

            return Ok(());
        }
    } else {
        send_ref_msg_404_fn("Failed to locate referenced message with attached MP3 file".into())
            .await
            .log_err()
            .ok();

        return Ok(());
    };

    // double check reference file attachment
    let attachment = if let Some(attachment) = ref_message.attachments.get(0) {
        const DEFAULT_STR: String = String::new();
        match attachment
            .content_type
            .as_ref()
            .unwrap_or(&DEFAULT_STR)
            .as_str()
        {
            "audio/mpeg" | "audio/mpeg3" | "x-mpeg-3" => attachment,
            unk_content_type => {
                let err_str = format!("Invalid CONTENT-TYPE({unk_content_type}). Expected 'audio/mpeg', 'audio/mpeg3', or 'x-mpeg-3'");

                component.create_response(&ctx.http(), CreateInteractionResponse::Message(CreateInteractionResponseMessage::new()
                .content(err_str.clone())))
                    .await.log_err_msg(format!("Failed to send response for unknown CONTENT-TYPE({unk_content_type}) for attached mp3 file message"))?;

                return Err(err_str.into());
            }
        }
    } else {
        return Err("Could not locate file attachment".into());
    };

    // have user fill out 'add sound' modal
    let response = component
        .quick_modal(
            &ctx,
            CreateQuickModal::new("Add Sounds")
                .field(
                    serenity::builder::CreateInputText::new(
                        serenity::all::InputTextStyle::Short,
                        "Name",
                        "sound_bot_sound_name_field",
                    )
                    .min_length(3)
                    .max_length(80)
                    .placeholder("Use The Force Luke"),
                )
                .field(
                    serenity::builder::CreateInputText::new(
                        serenity::all::InputTextStyle::Short,
                        "Tags",
                        "sound_bot_tags_field",
                    )
                    .max_length(1024)
                    .placeholder("star wars new hope"),
                ),
        )
        .await
        .log_err()?;

    let response = match response {
        Some(resp) => resp,
        None => return Ok(()),
    };

    // response
    //     .interaction
    //     .create_response(&ctx.http(), CreateInteractionResponse::Acknowledge)
    //     .await
    //     .log_err()?;

    let sound_name = &response.inputs[0];
    let sound_tags = Tags::from(response.inputs[1].clone());

    let temp_audio_file = crate::audio::download_audio_url_temp(&attachment.url)
        .await
        .log_err()?;

    crate::audio::AudioFileValidator::default()
        .max_audio_duration(data.config.max_audio_file_duration)
        .reject_uuid_files(false)
        .validate(&temp_audio_file)
        .log_err()?;

    // add sound track to sounds dir & update audio_table
    let audio_file = data.move_file_to_audio_dir(&temp_audio_file).log_err()?;
    let table = data.audio_table();
    table
        .insert_audio_row(
            db::audio_table::AudioTableRowInsertBuilder::new(sound_name.clone(), audio_file)
                .author_global_name(component.user.global_name.clone())
                .author_id(Some(component.user.id.into()))
                .author_name(Some(component.user.name.clone()))
                .tags(sound_tags)
                .build(),
        )
        .log_err()?;

    // update message to denote sound added
    response
        .interaction
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(format!("`{sound_name}` was added to soundbot!"))
                    .components(vec![]),
            ),
        )
        .await
        .log_err()?;

    Ok(())
}

pub async fn handle_ignore_mp3_file_btn(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    _data: &UserData,
) -> PoiseResult {
    log::info!("Handle ignore MP3 file button");

    // update message to denote sound added
    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(format!("Ignoring MP3 file. It was stupid anyway 😒"))
                    .components(vec![]),
            ),
        )
        .await
        .log_err()?;
    Ok(())
}

pub async fn handle_play_audio_btn(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
    audio_track_id: i64,
) -> PoiseResult {
    log::info!("Play Audio Button Pressed - '{audio_track_id}'");

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await
        .log_err_msg("Failed to create response for btn interaction")
        .ok();

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

pub async fn handle_paginate_btn(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
    button_id: PaginateId,
) -> PoiseResult {
    log::info!("paginate {button_id:?}");
    let conn = data.db_connection();

    let response_msg = match button_id {
        PaginateId::AllFirstPage(offset)
        | PaginateId::AllLastPage(offset)
        | PaginateId::AllPrevPage(offset)
        | PaginateId::AllNextPage(offset) => {
            let mut paginator = db::AudioTablePaginatorBuilder::all_template(conn)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            helpers::make_display_message(
                &mut paginator,
                helpers::DisplayType::All,
                None,
                data.config.enable_ephemeral_controls,
            )
            .log_err()?
        }
        PaginateId::MostPlayedFirstPage(offset)
        | PaginateId::MostPlayedLastPage(offset)
        | PaginateId::MostPlayedNextPage(offset)
        | PaginateId::MostPlayedPrevPage(offset) => {
            let mut paginator = db::AudioTablePaginatorBuilder::most_played_template(conn)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            helpers::make_display_message(
                &mut paginator,
                helpers::DisplayType::MostPlayed,
                None,
                data.config.enable_ephemeral_controls,
            )
            .log_err()?
        }
        PaginateId::RecentlyAddedFirstPage(offset)
        | PaginateId::RecentlyAddedLastPage(offset)
        | PaginateId::RecentlyAddedNextPage(offset)
        | PaginateId::RecentlyAddedPrevPage(offset) => {
            let mut paginator = db::AudioTablePaginatorBuilder::most_recently_added_template(conn)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            helpers::make_display_message(
                &mut paginator,
                helpers::DisplayType::RecentlyAdded,
                None,
                data.config.enable_ephemeral_controls,
            )
            .log_err()?
        }
        PaginateId::SearchFirstPage(offset, ref search)
        | PaginateId::SearchLastPage(offset, ref search)
        | PaginateId::SearchNextPage(offset, ref search)
        | PaginateId::SearchPrevPage(offset, ref search) => {
            let mut paginator = db::AudioTablePaginatorBuilder::search_template(conn, search)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            helpers::make_display_message(
                &mut paginator,
                helpers::DisplayType::Search,
                Some(search.clone()),
                data.config.enable_ephemeral_controls,
            )
            .log_err()?
        }
        PaginateId::PinnedFirstPage(offset)
        | PaginateId::PinnedLastPage(offset)
        | PaginateId::PinnedNextPage(offset)
        | PaginateId::PinnedPrevPage(offset) => {
            let mut paginator = db::AudioTablePaginatorBuilder::pinned_template(conn)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            helpers::make_display_message(
                &mut paginator,
                helpers::DisplayType::Pinned,
                None,
                data.config.enable_ephemeral_controls,
            )
            .log_err()?
        }
        PaginateId::Unknown(val) => {
            return Err(format!(
                "Unrecognized button custom_id for component interaction. Value={val}"
            )
            .into())
            .log_err();
        }
    };

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::UpdateMessage(response_msg.into()),
        )
        .await?;

    Ok(())
}

pub async fn handle_display_all_menu_select(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!("Displaying all sounds buttons as ActionRows grid...");
    let mut paginator = db::AudioTablePaginatorBuilder::all_template(data.db_connection())
        .page_limit(data.config.max_page_size)
        .build();

    let response_msg = helpers::make_display_message(
        &mut paginator,
        helpers::DisplayType::All,
        None,
        data.config.enable_ephemeral_controls,
    )
    .log_err()?;

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(response_msg.into()),
        )
        .await
        .log_err()?;

    component
        .create_followup(
            &ctx.http(),
            helpers::make_sound_controls_message(data.config.enable_ephemeral_controls).into(),
        )
        .await
        .log_err_msg("Failed sending soundbot controls")?;

    Ok(())
}

pub async fn handle_display_pinned_menu_select(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!("Displaying pinned sounds buttons as ActionRows grid...");

    let mut paginator = db::AudioTablePaginatorBuilder::pinned_template(data.db_connection())
        .page_limit(data.config.max_page_size)
        .build();

    let response_msg = helpers::make_display_message(
        &mut paginator,
        helpers::DisplayType::Pinned,
        None,
        data.config.enable_ephemeral_controls,
    )
    .log_err()?;

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(response_msg.into()),
        )
        .await
        .log_err()?;

    component
        .create_followup(
            &ctx.http(),
            helpers::make_sound_controls_message(data.config.enable_ephemeral_controls).into(),
        )
        .await
        .log_err_msg("Failed sending soundbot controls")?;

    Ok(())
}

pub async fn handle_display_recently_added_menu_select(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!("Displaying recently added sounds buttons as ActionRows grid...");

    let mut paginator =
        db::AudioTablePaginatorBuilder::most_recently_added_template(data.db_connection())
            .page_limit(data.config.max_page_size)
            .build();

    let response_msg = helpers::make_display_message(
        &mut paginator,
        helpers::DisplayType::RecentlyAdded,
        None,
        data.config.enable_ephemeral_controls,
    )
    .log_err()?;

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(response_msg.into()),
        )
        .await
        .log_err()?;

    component
        .create_followup(
            &ctx.http(),
            helpers::make_sound_controls_message(data.config.enable_ephemeral_controls).into(),
        )
        .await
        .log_err_msg("Failed sending soundbot controls")?;

    Ok(())
}

pub async fn handle_display_most_played_menu_select(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!("Displaying most played sounds buttons as ActionRows grid...");

    let mut paginator = db::AudioTablePaginatorBuilder::most_played_template(data.db_connection())
        .page_limit(data.config.max_page_size)
        .build();

    let response_msg = helpers::make_display_message(
        &mut paginator,
        helpers::DisplayType::MostPlayed,
        None,
        data.config.enable_ephemeral_controls,
    )
    .log_err()?;

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(response_msg.into()),
        )
        .await
        .log_err()?;

    component
        .create_followup(
            &ctx.http(),
            helpers::make_sound_controls_message(data.config.enable_ephemeral_controls).into(),
        )
        .await
        .log_err_msg("Failed sending soundbot controls")?;

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

            component
                .create_response(
                    &ctx.http(),
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(format!("### Playing `{track_name}`..."))
                            .components(helpers::make_soundbot_control_components(None)),
                    ),
                )
                .await?;

            let manager = helpers::songbird_get(&ctx).await;
            manager
                .play_audio(guild_id, channel_id, &audio_row.audio_file)
                .await
                .ok();
        }
        None => {
            component
                .create_response(
                    &ctx.http(),
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(format!("### No sounds present to play"))
                            .components(helpers::make_soundbot_control_components(None)),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_search_btn(
    ctx: &Context,
    _interaction: &Interaction,
    component: &ComponentInteraction,
    _framework: FrameworkContext<'_>,
    data: &UserData,
) -> PoiseResult {
    log::info!("Search button Pressed, creating search modal");
    let response = component
        .quick_modal(
            &ctx,
            CreateQuickModal::new("Search Sounds").field(
                serenity::builder::CreateInputText::new(
                    serenity::all::InputTextStyle::Short,
                    "Tags or Titles",
                    "soundbot_search_sound_modal_search_field",
                )
                .min_length(3)
                .max_length(80)
                .placeholder("star wars anakin"),
            ),
        )
        .await
        .log_err()?;

    if let Some(response) = response {
        let inputs = response.inputs;
        let search = &inputs[0];
        let search = search.trim();

        let mut paginator =
            db::AudioTablePaginatorBuilder::search_template(data.db_connection(), search)
                .page_limit(data.config.max_page_size)
                .build();

        let response_msg = helpers::make_display_message(
            &mut paginator,
            helpers::DisplayType::Search,
            Some(search.into()),
            data.config.enable_ephemeral_controls,
        )
        .log_err()?;

        response
            .interaction
            .create_response(
                &ctx.http(),
                CreateInteractionResponse::UpdateMessage(response_msg.into()),
            )
            .await
            .log_err()?;

        component
            .create_followup(
                &ctx.http(),
                helpers::make_sound_controls_message(data.config.enable_ephemeral_controls).into(),
            )
            .await
            .log_err_msg("Failed sending soundbot controls")?;
    } else {
        log::error!("Handle search button quick modal response was empty");
        return Ok(());
    }

    Ok(())
}
