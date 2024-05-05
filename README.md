# discord-soundboard-bot
A simple, unlimited audio track soundboard for discord, proxied via bot in voice channel.

## Overview

The `discord-soundboard-bot` allows users in a voice channel to play an unlimited length, unlimited number of audio tracks.

The simple steps to using the soundboard-bot are as follows...

- Join a voice channel
- Command the soundboard-bot to join the voice channel with the `sb:join` command
- Play desired sound using the `sb:play {track name}`

**Note** The soundboard-bot commands are typed in any text channel on the server.

- Refer to

## Dependencies

- [Songbird Dependencies](https://github.com/serenity-rs/songbird/tree/current#dependencies)
- [A Registered Discord Bot](https://discord.com/developers/docs/quick-start/getting-started)

## Commands

- `sb:join` - Join the voice channel that the author of the command is currently in
- `sb:leave` - Leave the voice channel that the author of the command is currently in
- `sb:list` - List audio track names that can be used in conjunction with the `sb:play` command
  - **note**: The list of audio track names are prefixed with `sb:play ` for easy copy pasting
- `sb:play {audio track name}` - Play an audio track name on the voice channel that the author of the message is in

## Environment variables

- `DISCORD_BOT_TOKEN` - The discord token. Available on the discord developer portal website.
- `DISCORD_BOT_APPLICATION_ID` - Bot application ID. Available on the discord developer portal website.
- `DISCORD_BOT_AUDIO_DIR` - **default**: `./audio` - The directory containing `.mp3` files to play.
- `DISCORD_BOT_COMMAND_PREFIX` - **default**: `sb:` - The command prefix when communicating to the bot from a discord text channel.
- `DISCORD_BOT_DOTENV_FILE` - **default**: `.env` - The dotenv file to load when launching the application
- `DISCORD_BOT_JOIN_AUDIO` - **default**: `{empty}` - The audio track to play when bot joins voice channel
- `DISCORD_BOT_LEAVE_AUDIO` - **default**: `{empty}` - The audio track to play when bot leaves voice channel
- `RUST_LOG` - Set log level for application (or speicific modules) in the application
  - Examples
    - `RUST_LOG=error`
      - Set log leve `error` for entire application
    - `RUST_LOG=soundboard_bot=info,serenity=error`
      - Set log level `info` for soundboard_bot, and `error` for serenity (crate)
