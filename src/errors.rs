use thiserror::Error;

#[allow(unused)]
#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Audio Track not found - {track}")]
    AudioTrackNotFound { track: String },
    #[error("Bot not in voice channel.")]
    NotInVoiceChannel,
}
