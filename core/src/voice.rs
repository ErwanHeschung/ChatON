//! Voice (spec § 11): voice channels in communities, and DM calls whose audio
//! is end-to-end encrypted with a key only the members have.
//!
//! Tokens come from the instance (NIP-98) and are always available; joining a
//! room needs the LiveKit SDK (feature `voice`), which the app enables.

use nostr::prelude::PublicKey;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};

use crate::session::{Session, http};
use crate::{Error, Result, media};

const WELL_KNOWN: &str = "/.well-known/nip29/livekit";
const CALL_PATH: &str = "/api/voice/call";

/// Where I'm talking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceTarget {
    Channel { community: String, channel: String },
    Call { room: Vec<PublicKey>, id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    Connected,
    Reconnecting,
}

/// The voice channel or call I'm in, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceState {
    pub target: VoiceTarget,
    pub connection: Connection,
    pub muted: bool,
    /// Hears nothing; the microphone is off too.
    pub deafened: bool,
    /// False: listening only (no microphone, or not allowed).
    pub microphone: bool,
    /// End-to-end encrypted (DM calls).
    pub encrypted: bool,
    /// Who is here, me included, each key once.
    pub participants: Vec<PublicKey>,
    pub speaking: Vec<PublicKey>,
}

/// A LiveKit server and a token for it (spec § 11).
#[derive(Debug, Deserialize)]
pub struct Token {
    pub server_url: String,
    pub participant_token: String,
}

impl Session {
    /// Whether the instance hosting `community` has voice.
    pub async fn voice_available(&self, community: &str) -> Result<bool> {
        let relay = self.relay_of(community).await?;
        let url = format!("{}{WELL_KNOWN}", media::http_base(&relay));
        Ok(http().get(url).send().await?.status() == StatusCode::NO_CONTENT)
    }

    pub async fn channel_token(&self, community: &str, channel: &str) -> Result<Token> {
        let relay = self.relay_of(community).await?;
        let url = format!("{}{WELL_KNOWN}/{channel}", media::http_base(&relay));
        token(self.nip98(Method::POST, &url, "{}".into()).await?).await
    }

    /// `base`: the caller's instance. The key never goes there.
    pub async fn call_token(&self, base: &str, id: &str, room: &[PublicKey]) -> Result<Token> {
        let body = serde_json::json!({ "id": id, "members": room }).to_string();
        let url = format!("{base}{CALL_PATH}");
        token(self.nip98(Method::POST, &url, body).await?).await
    }
}

async fn token(resp: reqwest::Response) -> Result<Token> {
    match resp.status() {
        StatusCode::CREATED => Ok(resp.json().await?),
        status => Err(status_error(status, resp.text().await.unwrap_or_default())),
    }
}

fn status_error(status: StatusCode, text: String) -> Error {
    match status {
        StatusCode::FORBIDDEN => Error::NotAllowed,
        StatusCode::NOT_FOUND => Error::VoiceDisabled,
        StatusCode::BAD_REQUEST => Error::Invalid(text),
        _ => Error::Server(format!("{status}: {text}")),
    }
}

#[cfg_attr(not(feature = "voice"), allow(dead_code))]
/// The keys behind LiveKit identities (`<hex>:<suffix>`), sorted, each once.
fn keys(identities: impl IntoIterator<Item = String>) -> Vec<PublicKey> {
    let mut all: Vec<PublicKey> = identities
        .into_iter()
        .filter_map(|id| PublicKey::from_hex(id.split(':').next()?).ok())
        .collect();
    all.sort();
    all.dedup();
    all
}

#[cfg(feature = "voice")]
pub(crate) use live::Slot;

#[cfg(feature = "voice")]
mod live {
    use std::sync::{Arc, Mutex};

    use livekit::e2ee::key_provider::{KeyProvider, KeyProviderOptions};
    use livekit::e2ee::{E2eeOptions, EncryptionType};
    use livekit::options::TrackPublishOptions;
    use livekit::prelude::*;
    use nostr::prelude::PublicKey;
    use tokio::sync::broadcast;

    use super::{Connection, Token, VoiceState, VoiceTarget, keys};
    use crate::community::Update;
    use crate::session::{Session, instance_url};
    use crate::{Error, Result, dm};

    /// The room I'm in, if any. Shared with the room's event task, which empties
    /// it when the server ends the session (e.g. I was removed).
    pub(crate) type Slot = Arc<tokio::sync::Mutex<Option<Active>>>;

    pub(crate) struct Active {
        room: Arc<Room>,
        // The track, not its publication: a full reconnect republishes the same track
        // under a new publication, and muting the old one would do nothing.
        mic: Option<LocalAudioTrack>,
        state: Arc<Mutex<VoiceState>>,
        events: tokio::task::JoinHandle<()>,
        // Plays the remote audio and records the microphone while kept.
        _audio: Option<livekit::PlatformAudio>,
    }

    impl Session {
        /// Joins a voice channel of `community`. Without `microphone` (none, or
        /// not allowed), I only listen.
        pub async fn join_voice_channel(
            &self,
            community: &str,
            channel: &str,
            microphone: bool,
        ) -> Result<VoiceState> {
            let token = self.channel_token(community, channel).await?;
            let target = VoiceTarget::Channel {
                community: community.into(),
                channel: channel.into(),
            };
            self.connect_voice(target, token, None, microphone).await
        }

        /// Calls `to` (one person or a group's members): invites them with a new
        /// id and audio key (NIP-17), then joins the call on my instance.
        pub async fn start_call(&self, to: &[PublicKey], microphone: bool) -> Result<VoiceState> {
            let room = dm::room(self.account().pubkey, to);
            let base = instance_url(&self.account().handle)
                .ok_or_else(|| Error::Invalid("handle without instance".into()))?;
            let id = hex::encode(random::<16>()?);
            let key = random::<32>()?;
            // Token first: no invite for a call that can't happen.
            let token = self.call_token(&base, &id, &room).await?;
            let invite = self
                .deliver(dm::wrap_call(&self.keys, &room, &id, &base, &key)?)
                .await?;
            // My copy reaches no listener (sent, not received): the app gets it here.
            drop(self.updates.send(Update::Dm { message: invite }));
            self.connect_voice(VoiceTarget::Call { room, id }, token, Some(key), microphone)
                .await
        }

        /// Joins the call `id` of the conversation `room`, from its invite.
        pub async fn join_call(
            &self,
            room: &[PublicKey],
            id: &str,
            microphone: bool,
        ) -> Result<VoiceState> {
            let messages = self.messages(room).await?;
            let invite = messages
                .iter()
                .rev()
                .find(|m| {
                    m.call
                        .as_ref()
                        .is_some_and(|c| c.id == id && c.invite.is_some())
                })
                .ok_or(Error::NotFound)?;
            if !self.invited_by_caller_instance(invite).await {
                return Err(Error::NotAllowed);
            }
            let Some(call) = invite.call.as_ref().and_then(|c| c.invite.as_ref()) else {
                return Err(Error::NotFound);
            };
            let room = invite.room.clone();
            let token = self.call_token(&call.url, id, &room).await?;
            let target = VoiceTarget::Call {
                room,
                id: id.into(),
            };
            self.connect_voice(target, token, Some(call.key), microphone)
                .await
        }

        /// Leaves the voice channel or call. The members get a `call-end` when the
        /// call is over: a 1:1 call as soon as one hangs up, a group call when the
        /// last one leaves (or nobody answered).
        pub async fn leave_voice(&self) -> Result<()> {
            self.hang_up(true).await
        }

        /// Someone's `call-end` for the call I'm in: it's over for me too, quietly.
        pub(crate) async fn call_ended(&self, id: &str) -> Result<()> {
            let mine = match self.voice.lock().await.as_ref() {
                Some(active) => matches!(&snapshot(&active.state).target,
                    VoiceTarget::Call { id: current, .. } if current == id),
                None => false,
            };
            if mine {
                self.hang_up(false).await
            } else {
                Ok(())
            }
        }

        async fn hang_up(&self, announce: bool) -> Result<()> {
            let Some(active) = self.voice.lock().await.take() else {
                return Ok(());
            };
            active.events.abort();
            let alone = active.room.remote_participants().is_empty();
            let target = snapshot(&active.state).target;
            drop(active.room.close().await);
            drop(active);
            drop(self.updates.send(Update::Voice { state: None }));
            if let VoiceTarget::Call { room, id } = target
                && announce
                && (alone || room.len() <= 2)
            {
                let end = self
                    .deliver(dm::wrap_call_end(&self.keys, &room, &id)?)
                    .await?;
                drop(self.updates.send(Update::Dm { message: end }));
            }
            Ok(())
        }

        pub async fn set_muted(&self, muted: bool) -> Result<VoiceState> {
            self.change(|st| st.muted = muted).await
        }

        /// Deafened: I hear nobody (unsubscribed) and my microphone is off.
        pub async fn set_deafened(&self, deafened: bool) -> Result<VoiceState> {
            self.change(|st| st.deafened = deafened).await
        }

        pub async fn voice_state(&self) -> Option<VoiceState> {
            Some(snapshot(&self.voice.lock().await.as_ref()?.state))
        }

        async fn change(&self, edit: impl FnOnce(&mut VoiceState)) -> Result<VoiceState> {
            let slot = self.voice.lock().await;
            let active = slot.as_ref().ok_or(Error::NotFound)?;
            let st = {
                let mut st = active.state.lock().map_err(|_| poisoned())?;
                edit(&mut st);
                st.clone()
            };
            apply(active, &st);
            drop(self.updates.send(Update::Voice {
                state: Some(st.clone()),
            }));
            Ok(st)
        }

        async fn connect_voice(
            &self,
            target: VoiceTarget,
            token: Token,
            key: Option<[u8; 32]>,
            microphone: bool,
        ) -> Result<VoiceState> {
            self.leave_voice().await?;
            // No audio device at all: still in the room (presence), but silent.
            let audio = livekit::PlatformAudio::new().ok();
            let mut options = RoomOptions::default();
            options.encryption = key.map(|k| E2eeOptions {
                encryption_type: EncryptionType::Gcm,
                key_provider: KeyProvider::with_shared_key(
                    KeyProviderOptions::default(),
                    k.to_vec(),
                ),
            });
            let (room, events) =
                Room::connect(&token.server_url, &token.participant_token, options)
                    .await
                    .map_err(|e| Error::Voice(e.to_string()))?;
            let room = Arc::new(room);
            let mut mic = None;
            if let Some(audio) = audio.as_ref().filter(|_| microphone)
                && audio.recording_devices().next().is_some()
            {
                let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
                let options = TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                };
                // A microphone that fails to open: listen only.
                mic = room
                    .local_participant()
                    .publish_track(LocalTrack::Audio(track.clone()), options)
                    .await
                    .ok()
                    .map(|_| track);
            }
            let state = Arc::new(Mutex::new(VoiceState {
                target,
                connection: Connection::Connected,
                muted: false,
                deafened: false,
                microphone: mic.is_some(),
                encrypted: key.is_some(),
                participants: participants(&room),
                speaking: Vec::new(),
            }));
            let task = tokio::spawn(run(
                room.clone(),
                events,
                state.clone(),
                self.voice.clone(),
                self.updates.clone(),
            ));
            let st = snapshot(&state);
            *self.voice.lock().await = Some(Active {
                room,
                mic,
                state,
                events: task,
                _audio: audio,
            });
            drop(self.updates.send(Update::Voice {
                state: Some(st.clone()),
            }));
            Ok(st)
        }
    }

    /// Follows the room: who's here, who speaks, the connection.
    async fn run(
        room: Arc<Room>,
        mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
        state: Arc<Mutex<VoiceState>>,
        slot: Slot,
        updates: broadcast::Sender<Update>,
    ) {
        while let Some(event) = events.recv().await {
            let st = {
                let Ok(mut st) = state.lock() else { return };
                match event {
                    RoomEvent::ParticipantConnected(_) | RoomEvent::ParticipantDisconnected(_) => {
                        st.participants = participants(&room);
                    }
                    RoomEvent::ActiveSpeakersChanged { speakers } => {
                        st.speaking = keys(speakers.iter().map(|p| p.identity().to_string()));
                    }
                    RoomEvent::TrackPublished { publication, .. } if st.deafened => {
                        publication.set_subscribed(false);
                        continue;
                    }
                    RoomEvent::Reconnecting => st.connection = Connection::Reconnecting,
                    RoomEvent::Reconnected => st.connection = Connection::Connected,
                    RoomEvent::Disconnected { .. } => break,
                    _ => continue,
                }
                st.clone()
            };
            drop(updates.send(Update::Voice { state: Some(st) }));
        }
        // Ended by the server, or the connection is lost for good.
        let mut slot = slot.lock().await;
        if slot.as_ref().is_some_and(|a| Arc::ptr_eq(&a.state, &state)) {
            *slot = None;
            drop(updates.send(Update::Voice { state: None }));
        }
    }

    /// Mute and deafen, applied to the room.
    fn apply(active: &Active, st: &VoiceState) {
        if let Some(mic) = &active.mic {
            match st.muted || st.deafened {
                true => mic.mute(),
                false => mic.unmute(),
            }
        }
        for participant in active.room.remote_participants().values() {
            for publication in participant.track_publications().values() {
                if publication.kind() == TrackKind::Audio {
                    publication.set_subscribed(!st.deafened);
                }
            }
        }
    }

    /// Me and the others, by key.
    fn participants(room: &Room) -> Vec<PublicKey> {
        let me = room.local_participant().identity().to_string();
        let others = room
            .remote_participants()
            .into_keys()
            .map(|id| id.to_string());
        keys(std::iter::once(me).chain(others))
    }

    fn snapshot(state: &Mutex<VoiceState>) -> VoiceState {
        match state.lock() {
            Ok(st) => st.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn poisoned() -> Error {
        Error::Voice("state lock poisoned".into())
    }

    fn random<const N: usize>() -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        getrandom::fill(&mut bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_errors_and_identities() {
        assert!(matches!(
            status_error(StatusCode::FORBIDDEN, String::new()),
            Error::NotAllowed
        ));
        assert!(matches!(
            status_error(StatusCode::NOT_FOUND, String::new()),
            Error::VoiceDisabled
        ));
        assert!(matches!(
            status_error(StatusCode::BAD_REQUEST, "x".into()),
            Error::Invalid(_)
        ));
        let mut ab = [
            nostr::prelude::Keys::generate().public_key(),
            nostr::prelude::Keys::generate().public_key(),
        ];
        ab.sort();
        let (a, b) = (ab[0].to_hex(), ab[1].to_hex());
        let got = keys([
            format!("{b}:aaaaaaaa"),
            format!("{a}:bbbbbbbb"),
            format!("{b}:cccccccc"),
            "not-a-key".into(),
        ]);
        let hex: Vec<String> = got.iter().map(|k| k.to_hex()).collect();
        assert_eq!(hex, [a, b]);
    }
}
