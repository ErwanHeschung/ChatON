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
    /// Who shares a screen, me included.
    pub sharing: Vec<PublicKey>,
    /// Whose screen I watch (one of `sharing`).
    pub watching: Option<PublicKey>,
}

/// A screen or window to share (`Session::screen_sources`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSource {
    pub id: u64,
    pub title: String,
    /// A whole screen, else a window.
    pub screen: bool,
    /// What it shows now: a small JPEG as a `data:` URL, if it could be captured.
    #[serde(default, skip_deserializing)]
    pub thumbnail: Option<String>,
}

/// Whether this platform can share its screen (desktop). All can watch.
pub fn can_share_screen() -> bool {
    cfg!(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    ))
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
pub use live::{Sink, screen_sources};

#[cfg(feature = "voice")]
mod live {
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use futures_core::Stream;
    use livekit::e2ee::key_provider::{KeyProvider, KeyProviderOptions};
    use livekit::e2ee::{E2eeOptions, EncryptionType};
    use livekit::options::{TrackPublishOptions, screenshare};
    use livekit::prelude::*;
    use livekit::webrtc::video_frame::{
        I420Buffer, VideoBuffer, VideoFormatType, VideoFrame, VideoRotation,
    };
    use livekit::webrtc::video_source::native::NativeVideoSource;
    use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
    use livekit::webrtc::video_stream::native::NativeVideoStream;
    use livekit::webrtc::video_track::RtcVideoTrack;
    use nostr::prelude::PublicKey;
    use tokio::sync::broadcast;

    use super::{Connection, ScreenSource, Token, VoiceState, VoiceTarget, keys};
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
        /// Set: my screen share's capture thread stops.
        share: Option<Arc<AtomicBool>>,
        /// Where the frames of the screen I watch go.
        sink: Arc<Mutex<Option<Sink>>>,
    }

    impl Drop for Active {
        fn drop(&mut self) {
            if let Some(stop) = &self.share {
                stop.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Receives the screen I watch, as JPEG images.
    pub type Sink = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

    /// One image every 66 ms: enough for a screen, not for a camera.
    const FRAME: Duration = Duration::from_millis(66);
    /// Wider screens are scaled down to this.
    const MAX_WIDTH: u32 = 1920;

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
                .deliver(dm::wrap_call(
                    &self.keys,
                    Some(&self.account().handle),
                    &room,
                    &id,
                    &base,
                    &key,
                )?)
                .await?;
            // My copy reaches no listener (sent, not received): the app gets it here.
            drop(self.updates.send(Update::Dm {
                message: invite,
                request: false,
            }));
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
                    .deliver(dm::wrap_call_end(
                        &self.keys,
                        Some(&self.account().handle),
                        &room,
                        &id,
                    )?)
                    .await?;
                drop(self.updates.send(Update::Dm {
                    message: end,
                    request: false,
                }));
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

        /// Shares a screen or window from `screen_sources`. None: the OS asks
        /// (Wayland's portal, macOS's picker). The state follows by `Update::Voice`.
        pub async fn start_screen_share(&self, source: Option<ScreenSource>) -> Result<()> {
            if !super::can_share_screen() {
                return Err(Error::Voice("screen sharing is desktop only".into()));
            }
            self.share(Capture::Screen(source)).await
        }

        /// A moving test pattern instead of a screen (tests).
        #[doc(hidden)]
        pub async fn share_test_pattern(&self) -> Result<()> {
            self.share(Capture::Pattern).await
        }

        pub async fn stop_screen_share(&self) -> Result<()> {
            let mut slot = self.voice.lock().await;
            let Some(active) = slot.as_mut() else {
                return Ok(());
            };
            if let Some(stop) = active.share.take() {
                stop.store(true, Ordering::Relaxed);
            }
            unpublish_screen(&active.room).await;
            Ok(())
        }

        /// Watches `who`'s screen, its images to `sink`; None stops. Only the
        /// screen I watch is downloaded.
        pub async fn watch_screen(&self, who: Option<(PublicKey, Sink)>) -> Result<VoiceState> {
            let (key, sink) = who.unzip();
            let slot = self.voice.lock().await;
            let active = slot.as_ref().ok_or(Error::NotFound)?;
            *active.sink.lock().map_err(|_| poisoned())? = sink.clone();
            let st = {
                let mut st = active.state.lock().map_err(|_| poisoned())?;
                st.watching = key.filter(|k| st.sharing.contains(k));
                st.clone()
            };
            apply(active, &st);
            // Mine, or already downloaded (watched before): no subscription event.
            if let (Some(key), Some(sink)) = (st.watching, sink) {
                let local = active.room.local_participant();
                let mine = keys([local.identity().to_string()]).pop() == Some(key);
                let tracks: Vec<RtcVideoTrack> = if mine {
                    local
                        .track_publications()
                        .into_values()
                        .filter(|p| p.source() == TrackSource::Screenshare)
                        .filter_map(|p| match p.track() {
                            Some(LocalTrack::Video(track)) => Some(track.rtc_track()),
                            _ => None,
                        })
                        .collect()
                } else {
                    screens(&active.room)
                        .into_iter()
                        .filter(|(author, _)| *author == key)
                        .filter_map(|(_, p)| match p.track() {
                            Some(RemoteTrack::Video(track)) => Some(track.rtc_track()),
                            _ => None,
                        })
                        .collect()
                };
                for track in tracks {
                    tokio::spawn(stream(
                        track,
                        key,
                        active.state.clone(),
                        sink.clone(),
                        active.sink.clone(),
                    ));
                }
            }
            drop(self.updates.send(Update::Voice {
                state: Some(st.clone()),
            }));
            Ok(st)
        }

        async fn share(&self, capture: Capture) -> Result<()> {
            self.stop_screen_share().await?;
            let room = match self.voice.lock().await.as_ref() {
                Some(active) => active.room.clone(),
                None => return Err(Error::NotFound),
            };
            let source = NativeVideoSource::new(
                VideoResolution {
                    width: MAX_WIDTH,
                    height: 1080,
                },
                true,
            );
            let stop = Arc::new(AtomicBool::new(false));
            let (ready, captured) = tokio::sync::oneshot::channel();
            let (feed, ended) = (source.clone(), stop.clone());
            let (runtime, shared) = (tokio::runtime::Handle::current(), room.clone());
            std::thread::spawn(move || {
                capture.run(&feed, &ended, ready);
                // Ended by the capturer (refused, window closed): unshare.
                if !ended.load(Ordering::Relaxed) {
                    runtime.spawn(async move { unpublish_screen(&shared).await });
                }
            });
            // Published once an image came: a capture that can't start (no portal,
            // picker cancelled) says so instead of showing an empty screen.
            if captured.await != Ok(true) {
                stop.store(true, Ordering::Relaxed);
                return Err(Error::ScreenCapture);
            }
            let mut slot = self.voice.lock().await;
            let Some(active) = slot.as_mut().filter(|a| Arc::ptr_eq(&a.room, &room)) else {
                stop.store(true, Ordering::Relaxed);
                return Err(Error::NotFound);
            };
            let track =
                LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(source));
            let options = TrackPublishOptions {
                source: TrackSource::Screenshare,
                simulcast: false,
                video_encoding: Some(screenshare::H1080_FPS15.encoding),
                ..Default::default()
            };
            if let Err(e) = room
                .local_participant()
                .publish_track(LocalTrack::Video(track), options)
                .await
            {
                stop.store(true, Ordering::Relaxed);
                return Err(Error::Voice(e.to_string()));
            }
            active.share = Some(stop);
            Ok(())
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
            // `apply` subscribes: audio unless deafened, a screen only when watched.
            options.auto_subscribe = false;
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
                sharing: sharing(&room),
                watching: None,
            }));
            let sink = Arc::new(Mutex::new(None));
            let task = tokio::spawn(run(
                room.clone(),
                events,
                state.clone(),
                sink.clone(),
                self.voice.clone(),
                self.updates.clone(),
            ));
            let st = snapshot(&state);
            let active = Active {
                room,
                mic,
                state,
                events: task,
                _audio: audio,
                share: None,
                sink,
            };
            // Who was already here.
            apply(&active, &st);
            *self.voice.lock().await = Some(active);
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
        sink: Arc<Mutex<Option<Sink>>>,
        slot: Slot,
        updates: broadcast::Sender<Update>,
    ) {
        while let Some(event) = events.recv().await {
            let st = {
                let Ok(mut st) = state.lock() else { return };
                match event {
                    RoomEvent::ParticipantConnected(_)
                    | RoomEvent::ParticipantDisconnected(_)
                    | RoomEvent::TrackPublished { .. }
                    | RoomEvent::TrackUnpublished { .. }
                    | RoomEvent::LocalTrackPublished { .. }
                    | RoomEvent::LocalTrackUnpublished { .. } => {
                        st.participants = participants(&room);
                        st.sharing = sharing(&room);
                        st.watching = st.watching.filter(|k| st.sharing.contains(k));
                        subscribe(&room, &st);
                    }
                    RoomEvent::TrackSubscribed {
                        track: RemoteTrack::Video(track),
                        publication,
                        participant,
                    } => {
                        let author = keys([participant.identity().to_string()]).pop();
                        let to = sink.lock().ok().and_then(|s| s.clone());
                        if publication.source() == TrackSource::Screenshare
                            && let (Some(key), Some(to)) = (author, to)
                            && st.watching == Some(key)
                        {
                            tokio::spawn(stream(
                                track.rtc_track(),
                                key,
                                state.clone(),
                                to,
                                sink.clone(),
                            ));
                        }
                        continue;
                    }
                    RoomEvent::ActiveSpeakersChanged { speakers } => {
                        st.speaking = keys(speakers.iter().map(|p| p.identity().to_string()));
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

    /// Mute, deafen and the watched screen, applied to the room.
    fn apply(active: &Active, st: &VoiceState) {
        if let Some(mic) = &active.mic {
            match st.muted || st.deafened {
                true => mic.mute(),
                false => mic.unmute(),
            }
        }
        subscribe(&active.room, st);
    }

    /// Downloads what I hear and the screen I watch, nothing else.
    fn subscribe(room: &Room, st: &VoiceState) {
        for (id, participant) in room.remote_participants() {
            let Some(author) = keys([id.to_string()]).pop() else {
                continue;
            };
            for publication in participant.track_publications().values() {
                let want = wants(publication.source(), &author, st);
                let desired = publication.subscription_status()
                    != livekit::publication::SubscriptionStatus::Unsubscribed;
                if want != desired {
                    publication.set_subscribed(want);
                }
            }
        }
    }

    pub(super) fn wants(source: TrackSource, author: &PublicKey, st: &VoiceState) -> bool {
        let watched = st.watching.as_ref() == Some(author);
        match source {
            TrackSource::Screenshare => watched,
            TrackSource::ScreenshareAudio => watched && !st.deafened,
            TrackSource::Camera => false,
            TrackSource::Microphone | TrackSource::Unknown => !st.deafened,
        }
    }

    /// Who shares a screen, me included.
    fn sharing(room: &Room) -> Vec<PublicKey> {
        let local = room.local_participant();
        let me = local
            .track_publications()
            .values()
            .any(|p| p.source() == TrackSource::Screenshare)
            .then(|| local.identity().to_string());
        let others = screens(room).into_iter().map(|(k, _)| k.to_hex());
        keys(me.into_iter().chain(others))
    }

    /// The others' screen shares, with their author.
    fn screens(room: &Room) -> Vec<(PublicKey, RemoteTrackPublication)> {
        let mut all = Vec::new();
        for (id, participant) in room.remote_participants() {
            let Some(author) = keys([id.to_string()]).pop() else {
                continue;
            };
            for publication in participant.track_publications().into_values() {
                if publication.source() == TrackSource::Screenshare {
                    all.push((author, publication));
                }
            }
        }
        all
    }

    async fn unpublish_screen(room: &Room) {
        let local = room.local_participant();
        for (sid, publication) in local.track_publications() {
            if publication.source() == TrackSource::Screenshare {
                drop(local.unpublish_track(&sid).await);
            }
        }
    }

    /// Sends `track`'s images to `to` while I watch `key` with that sink.
    async fn stream(
        track: RtcVideoTrack,
        key: PublicKey,
        state: Arc<Mutex<VoiceState>>,
        to: Sink,
        current: Arc<Mutex<Option<Sink>>>,
    ) {
        let mut frames = NativeVideoStream::new(track);
        let mut last: Option<Instant> = None;
        loop {
            // A stopped track sends nothing more: check now and then.
            let next = std::future::poll_fn(|cx| Pin::new(&mut frames).poll_next(cx));
            let next = tokio::time::timeout(Duration::from_secs(1), next).await;
            let still = snapshot(&state).watching == Some(key)
                && current
                    .lock()
                    .ok()
                    .and_then(|c| c.clone())
                    .is_some_and(|c| Arc::ptr_eq(&c, &to));
            let frame = match next {
                _ if !still => break,
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(_) => continue,
            };
            if last.is_some_and(|t| t.elapsed() < FRAME) {
                continue;
            }
            last = Some(Instant::now());
            // The newest image waits in the stream meanwhile; older ones are dropped.
            let encoded = tokio::task::spawn_blocking(move || jpeg(frame.buffer.as_ref(), 80));
            if let Ok(Some(jpeg)) = encoded.await {
                to(jpeg);
            }
        }
        frames.close();
    }

    /// The image as a JPEG for the webview.
    fn jpeg(buffer: &dyn VideoBuffer, quality: u8) -> Option<Vec<u8>> {
        let (w, h) = (buffer.width(), buffer.height());
        let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) else {
            return None;
        };
        let mut rgba = vec![0u8; w as usize * h as usize * 4];
        // libyuv's ABGR is R, G, B, A in memory.
        buffer.to_argb(VideoFormatType::ABGR, &mut rgba, w * 4, w as i32, h as i32);
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, quality)
            .encode(&rgba, w16, h16, jpeg_encoder::ColorType::Rgba)
            .ok()?;
        Some(out)
    }

    /// Told once: true at the first image, false if the capture can't start.
    type Ready = tokio::sync::oneshot::Sender<bool>;

    /// How long the OS picker may wait for a choice.
    #[cfg_attr(target_os = "android", allow(dead_code))]
    const PICKING: Duration = Duration::from_secs(60);

    /// What a screen share sends.
    enum Capture {
        #[cfg_attr(target_os = "android", allow(dead_code))]
        Screen(Option<ScreenSource>),
        Pattern,
    }

    impl Capture {
        /// Feeds `source` until `stop`, or until the capturer gives up.
        fn run(self, source: &NativeVideoSource, stop: &AtomicBool, ready: Ready) {
            match self {
                #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
                Capture::Screen(chosen) => desktop::capture(chosen, source, stop, ready),
                #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
                Capture::Screen(_) => {
                    let _ = ready.send(false);
                }
                Capture::Pattern => {
                    let _ = ready.send(true);
                    let mut i = 0u32;
                    while !stop.load(Ordering::Relaxed) {
                        let (w, h) = (1280, 720);
                        let mut buffer = I420Buffer::new(w, h);
                        let (y, u, v) = buffer.data_mut();
                        for (n, px) in y.iter_mut().enumerate() {
                            *px = ((n as u32 % w + i * 8) % 256) as u8;
                        }
                        u.fill(128);
                        v.fill(128);
                        source
                            .capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer));
                        i += 1;
                        std::thread::sleep(FRAME);
                    }
                }
            }
        }
    }

    /// Hands a captured image (ARGB words: B, G, R, A in memory) to the encoder.
    #[cfg_attr(target_os = "android", allow(dead_code))]
    fn push(source: &NativeVideoSource, argb: &[u8], stride: u32, w: u32, h: u32) {
        let buffer = fit(i420(argb, stride, w, h), MAX_WIDTH);
        source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer));
    }

    #[cfg_attr(target_os = "android", allow(dead_code))]
    fn i420(argb: &[u8], stride: u32, w: u32, h: u32) -> I420Buffer {
        let mut buffer = I420Buffer::new(w, h);
        let (sy, su, sv) = buffer.strides();
        let (y, u, v) = buffer.data_mut();
        livekit::webrtc::native::yuv_helper::argb_to_i420(
            argb, stride, y, sy, u, su, v, sv, w as i32, h as i32,
        );
        buffer
    }

    /// Scaled down to `width` if wider.
    #[cfg_attr(target_os = "android", allow(dead_code))]
    fn fit(mut buffer: I420Buffer, width: u32) -> I420Buffer {
        let (w, h) = (buffer.width(), buffer.height());
        if w <= width {
            return buffer;
        }
        let height = ((h * width / w) & !1).max(2);
        buffer.scale(width as i32, height as i32)
    }

    /// Screens and windows to share. Empty where the OS asks itself (Wayland's
    /// portal, macOS's picker): start with None.
    pub async fn screen_sources() -> Vec<ScreenSource> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        return tokio::task::spawn_blocking(desktop::sources)
            .await
            .unwrap_or_default();
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        Vec::new()
    }

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    mod desktop {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        use livekit::webrtc::desktop_capturer::{
            CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
        };
        use livekit::webrtc::video_source::native::NativeVideoSource;

        use std::sync::Mutex;
        use std::time::Instant;

        use std::time::Duration;

        use livekit::webrtc::desktop_capturer::CaptureSource;
        use livekit::webrtc::video_frame::I420Buffer;

        use super::{FRAME, PICKING, Ready, ScreenSource, fit, i420, jpeg, push};

        /// The OS shows its own picker: Wayland (portal), macOS.
        /// (macOS; Wayland when the app runs on it: `GDK_BACKEND=x11` lists X11 windows.)
        fn os_picker() -> bool {
            let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
                && std::env::var("GDK_BACKEND").map_or(true, |b| !b.starts_with("x11"));
            cfg!(target_os = "macos") || (cfg!(target_os = "linux") && wayland)
        }

        fn capturer(kind: DesktopCaptureSourceType) -> Option<DesktopCapturer> {
            let mut options = DesktopCapturerOptions::new(kind);
            options.set_include_cursor(true);
            DesktopCapturer::new(options)
        }

        pub(super) fn sources() -> Vec<ScreenSource> {
            if os_picker() {
                return Vec::new();
            }
            let mut found = Vec::new();
            for (kind, screen) in [
                (DesktopCaptureSourceType::Screen, true),
                (DesktopCaptureSourceType::Window, false),
            ] {
                if let Some(capturer) = capturer(kind) {
                    found.extend(
                        capturer
                            .get_source_list()
                            .into_iter()
                            .map(|s| (kind, screen, s)),
                    );
                }
            }
            // One capturer per source, all at once: the picker waits for the slowest.
            std::thread::scope(|scope| {
                let shots: Vec<_> = found
                    .into_iter()
                    .map(|(kind, screen, source)| {
                        let (id, title) = (source.id(), source.title());
                        let shot = scope.spawn(move || thumbnail(kind, source));
                        (id, title, screen, shot)
                    })
                    .collect();
                shots
                    .into_iter()
                    .map(|(id, title, screen, shot)| ScreenSource {
                        id,
                        title,
                        screen,
                        thumbnail: shot.join().ok().flatten(),
                    })
                    .collect()
            })
        }

        /// One image of `source`, small, as a `data:` URL.
        fn thumbnail(kind: DesktopCaptureSourceType, source: CaptureSource) -> Option<String> {
            use base64::Engine;
            let mut capturer = capturer(kind)?;
            let shot: Arc<Mutex<Option<I420Buffer>>> = Arc::default();
            let out = shot.clone();
            capturer.start_capture(Some(source), move |result| {
                if let Ok(frame) = result
                    && let (Ok(w), Ok(h)) =
                        (u32::try_from(frame.width()), u32::try_from(frame.height()))
                    && w > 0
                    && h > 0
                    && let Ok(mut out) = out.lock()
                {
                    *out = Some(i420(frame.data(), frame.stride(), w, h));
                }
            });
            for _ in 0..10 {
                capturer.capture_frame();
                if shot.lock().is_ok_and(|s| s.is_some()) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let image = shot.lock().ok()?.take()?;
            let jpeg = jpeg(&fit(image, 320), 70)?;
            Some(format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(jpeg)
            ))
        }

        pub(super) fn capture(
            chosen: Option<ScreenSource>,
            source: &NativeVideoSource,
            stop: &AtomicBool,
            ready: Ready,
        ) {
            #[cfg(target_os = "linux")]
            let picked = match os_picker() {
                true => DesktopCaptureSourceType::Generic,
                false => DesktopCaptureSourceType::Screen,
            };
            #[cfg(not(target_os = "linux"))]
            let picked = DesktopCaptureSourceType::Screen;
            let kind = match &chosen {
                Some(c) if c.screen => DesktopCaptureSourceType::Screen,
                Some(_) => DesktopCaptureSourceType::Window,
                None => picked,
            };
            let Some(mut capturer) = capturer(kind) else {
                let _ = ready.send(false);
                return;
            };
            let target = match &chosen {
                Some(c) => {
                    let found = capturer
                        .get_source_list()
                        .into_iter()
                        .find(|s| s.id() == c.id);
                    if found.is_none() {
                        let _ = ready.send(false);
                        return;
                    }
                    found
                }
                None => None,
            };
            let ready = Arc::new(Mutex::new(Some(ready)));
            let failed = Arc::new(AtomicBool::new(false));
            let (out, gave_up, first) = (source.clone(), failed.clone(), ready.clone());
            capturer.start_capture(target, move |result| match result {
                Ok(frame) => {
                    let (w, h) = (frame.width(), frame.height());
                    if let (Ok(w), Ok(h)) = (u32::try_from(w), u32::try_from(h))
                        && w > 0
                        && h > 0
                    {
                        push(&out, frame.data(), frame.stride(), w, h);
                        tell(&first, true);
                    }
                }
                Err(CaptureError::Permanent) => gave_up.store(true, Ordering::Relaxed),
                Err(CaptureError::Temporary) => {}
            });
            let started = Instant::now();
            while !stop.load(Ordering::Relaxed) && !failed.load(Ordering::Relaxed) {
                let waiting = ready.lock().is_ok_and(|r| r.is_some());
                if waiting && started.elapsed() > PICKING {
                    break;
                }
                capturer.capture_frame();
                std::thread::sleep(FRAME);
            }
            tell(&ready, false);
        }

        /// The first word on `ready` wins.
        fn tell(ready: &Mutex<Option<Ready>>, ok: bool) {
            if let Some(ready) = ready.lock().ok().and_then(|mut r| r.take()) {
                let _ = ready.send(ok);
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

    #[cfg(feature = "voice")]
    #[test]
    fn downloads_what_i_hear_and_the_screen_i_watch() {
        use livekit::prelude::TrackSource as S;
        let (a, b) = (
            nostr::prelude::Keys::generate().public_key(),
            nostr::prelude::Keys::generate().public_key(),
        );
        let mut st = VoiceState {
            target: VoiceTarget::Call {
                room: vec![a, b],
                id: "x".into(),
            },
            connection: Connection::Connected,
            muted: false,
            deafened: false,
            microphone: true,
            encrypted: true,
            participants: vec![a, b],
            speaking: vec![],
            sharing: vec![a, b],
            watching: Some(a),
        };
        assert!(live::wants(S::Microphone, &b, &st));
        assert!(live::wants(S::Screenshare, &a, &st));
        assert!(!live::wants(S::Screenshare, &b, &st));
        assert!(!live::wants(S::Camera, &a, &st));
        st.deafened = true;
        st.watching = None;
        assert!(!live::wants(S::Microphone, &b, &st));
        assert!(!live::wants(S::Screenshare, &a, &st));
    }

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
