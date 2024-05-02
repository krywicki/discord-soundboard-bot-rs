# discord-soundboard-bot
A simple, unlimited audio track soundboard for discord, proxied via bot in voice channel.

# Overview

The `discord-soundboard-bot` allows users in a voice channel to play an unlimited length, unlimited number of audio tracks.

The simple steps to using the soundboard-bot are as follows...

- Join a voice channel
- Command the soundboard-bot to join the voice channel with the `sb:join` command
- Play desired sound using the `sb:play {track name}`

**Note** The soundboard-bot commands are typed in any text channel on the server.


# Dependencies

View [Songbird Dependencies](https://github.com/serenity-rs/songbird/tree/current#dependencies)

# Commands

- `sb:join` - Join the voice channel that the author of the command is currently in
- `sb:leave` - Leave the voice channel that the author of the command is currently in
- `sb:list` - List audio track names that can be used in conjunction with the `sb:play` command
  - **note**: The list of audio track names are prefixed with `sb:play ` for easy copy pasting
- `sb:play {audio track name}` - Play an audio track name on the voice channel that the author of the message is in
