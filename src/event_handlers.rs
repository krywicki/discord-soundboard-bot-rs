use serenity::all::{
    CacheHttp, ComponentInteraction, ComponentInteractionDataKind, Context, CreateActionRow,
    CreateButton, CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
    CreateQuickModal, FullEvent, Interaction, ReactionType, VoiceState,
};

use crate::{
    commands::PoiseResult,
    common::{LogResult, UserData},
    db::{self, paginators::AudioTablePaginatorBuilder, AudioTable, SettingsTable, Table},
    helpers::{
        self, check_msg, ButtonCustomId, DisplayMenuItemCustomId, PaginateId, SongbirdHelper,
    },
    vars::ACTION_ROWS_LIMIT,
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

    let (mut paginator, content, paginate_btns) = match button_id {
        PaginateId::AllPrevPage(offset) | PaginateId::AllNextPage(offset) => {
            let paginator = db::AudioTablePaginatorBuilder::all_template(conn)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            let paginate_info = paginator.pageinate_info()?;
            let content =
                helpers::make_display_title(helpers::PaginateType::All, &paginate_info, None);
            let paginate_btns =
                helpers::make_paginate_controls(helpers::PaginateType::All, &paginate_info, None);

            (paginator, content, paginate_btns)
        }
        PaginateId::MostPlayedNextPage(offset) | PaginateId::MostPlayedPrevPage(offset) => {
            let paginator = db::AudioTablePaginatorBuilder::most_played_template(conn)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            let paginate_info = paginator.pageinate_info()?;
            let content = helpers::make_display_title(
                helpers::PaginateType::MostPlayed,
                &paginate_info,
                None,
            );

            let paginate_btns = helpers::make_paginate_controls(
                helpers::PaginateType::MostPlayed,
                &paginate_info,
                None,
            );

            (paginator, content, paginate_btns)
        }
        PaginateId::RecentlyAddedNextPage(offset) | PaginateId::RecentlyAddedPrevPage(offset) => {
            let paginator = db::AudioTablePaginatorBuilder::most_recently_added_template(conn)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            let paginate_info = paginator.pageinate_info()?;
            let content = helpers::make_display_title(
                helpers::PaginateType::RecentlyAdded,
                &paginate_info,
                None,
            );

            let paginate_btns = helpers::make_paginate_controls(
                helpers::PaginateType::RecentlyAdded,
                &paginate_info,
                None,
            );

            (paginator, content, paginate_btns)
        }
        PaginateId::SearchNextPage(offset, ref search)
        | PaginateId::SearchPrevPage(offset, ref search) => {
            let paginator = db::AudioTablePaginatorBuilder::search_template(conn, search)
                .page_limit(data.config.max_page_size)
                .offset(offset)
                .build();

            let paginate_info = paginator.pageinate_info()?;
            let content = helpers::make_display_title(
                helpers::PaginateType::Search,
                &paginate_info,
                Some(search.clone()),
            );

            let paginate_btns = helpers::make_paginate_controls(
                helpers::PaginateType::Search,
                &paginate_info,
                Some(search.clone()),
            );

            (paginator, content, paginate_btns)
        }
        PaginateId::Unknown(val) => {
            return Err(format!(
                "Unrecognized button custom_id for component interaction. Value={val}"
            )
            .into())
            .log_err();
        }
    };

    let mut action_rows: Vec<_> = paginator
        .next_page()?
        .chunks(5)
        .map(helpers::make_action_row)
        .collect();
    action_rows.push(paginate_btns);

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(action_rows),
            ),
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

    let paginate_info = paginator.pageinate_info()?;
    let content = helpers::make_display_title(helpers::PaginateType::All, &paginate_info, None);
    let paginate_ctrls =
        helpers::make_paginate_controls(helpers::PaginateType::All, &paginate_info, None);

    let mut btn_grid: Vec<_> = paginator
        .next_page()?
        .chunks(5)
        .map(helpers::make_action_row)
        .collect();

    btn_grid.push(paginate_ctrls);

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(btn_grid),
            ),
        )
        .await
        .log_err()?;

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

    let paginate_info = paginator.pageinate_info()?;
    let content = helpers::make_display_title(helpers::PaginateType::Pinned, &paginate_info, None);
    let paginate_ctrls =
        helpers::make_paginate_controls(helpers::PaginateType::Pinned, &paginate_info, None);
    let mut btn_grid: Vec<_> = paginator
        .next_page()?
        .chunks(5)
        .map(helpers::make_action_row)
        .collect();

    btn_grid.push(paginate_ctrls);

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(btn_grid),
            ),
        )
        .await
        .log_err()?;

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

    let paginate_info = paginator.pageinate_info()?;
    let content =
        helpers::make_display_title(helpers::PaginateType::RecentlyAdded, &paginate_info, None);
    let paginate_ctrls =
        helpers::make_paginate_controls(helpers::PaginateType::RecentlyAdded, &paginate_info, None);
    let mut btn_grid: Vec<_> = paginator
        .next_page()?
        .chunks(5)
        .map(helpers::make_action_row)
        .collect();

    btn_grid.push(paginate_ctrls);

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(btn_grid),
            ),
        )
        .await
        .log_err()?;

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

    let paginate_info = paginator.pageinate_info()?;
    let content =
        helpers::make_display_title(helpers::PaginateType::MostPlayed, &paginate_info, None);
    let paginate_ctrls =
        helpers::make_paginate_controls(helpers::PaginateType::MostPlayed, &paginate_info, None);
    let mut btn_grid: Vec<_> = paginator
        .next_page()?
        .chunks(5)
        .map(helpers::make_action_row)
        .collect();

    btn_grid.push(paginate_ctrls);

    component
        .create_response(
            &ctx.http(),
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .components(btn_grid),
            ),
        )
        .await
        .log_err()?;

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
                            .components(helpers::make_soundbot_control_components()),
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
                            .components(helpers::make_soundbot_control_components()),
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

    let channel_id = component.channel_id;

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
        response
            .interaction
            .create_response(&ctx.http(), CreateInteractionResponse::Acknowledge)
            .await?;

        let inputs = response.inputs;
        let search = &inputs[0];
        let search = search.trim();

        let mut paginator = db::AudioTablePaginatorBuilder::pinned_template(data.db_connection())
            .page_limit(data.config.max_page_size)
            .build();

        let paginate_info = paginator.pageinate_info()?;
        let content = helpers::make_display_title(
            helpers::PaginateType::Search,
            &paginate_info,
            Some(search.into()),
        );
        let paginate_ctrls = helpers::make_paginate_controls(
            helpers::PaginateType::Search,
            &paginate_info,
            Some(search.into()),
        );
        let mut btn_grid: Vec<_> = paginator
            .next_page()?
            .chunks(5)
            .map(helpers::make_action_row)
            .collect();

        btn_grid.push(paginate_ctrls);

        component
            .create_response(
                &ctx.http(),
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(content)
                        .components(btn_grid),
                ),
            )
            .await
            .log_err()?;

        //
        let paginator = AudioTablePaginatorBuilder::new(data.db_connection())
            .fts_filter(Some(search.into()))
            .page_limit(ACTION_ROWS_LIMIT)
            .build();

        check_msg(
            channel_id
                .send_message(
                    &ctx.http(),
                    CreateMessage::new().content(format!("### Search Results for `{search}`...")),
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
    } else {
        log::error!("Handle search button quick modal response was empty");
        return Ok(());
    }

    Ok(())
}
