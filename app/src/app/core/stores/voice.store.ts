import { computed, inject } from '@angular/core';
import {
  patchState,
  signalStore,
  withComputed,
  withHooks,
  withMethods,
  withState,
} from '@ngrx/signals';
import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CmdError } from '@models/Auth.model';
import { DmMessage, roomKey } from '@models/Dm.model';
import { ScreenSource, VoiceState, VoiceUpdate } from '@models/Voice.model';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { ShellService } from '@services/shell.service';

const code = (e: unknown): CmdError['code'] => (e as CmdError).code ?? 'internal';

/** The voice channel or call I'm in, and the call ringing. Audio and keys stay in core. */
export const VoiceStore = signalStore(
  { providedIn: 'root' },
  withState({
    state: null as VoiceState | null,
    /** An incoming call, until `until` (seconds), its end, or an answer. */
    ring: null as { message: DmMessage; until: number } | null,
    /** Community id → its instance has voice. */
    available: {} as Record<string, boolean>,
    /** Joining or leaving. */
    busy: false,
    error: null as CmdError['code'] | null,
    /** This device can share its screen (desktop). */
    canShare: false,
    /** Screens and windows to pick from; null: no picker open. */
    sources: null as ScreenSource[] | null,
  }),
  withComputed((store, user = inject(ConnectedUserStore)) => {
    const target = computed(() => store.state()?.target);
    return {
      /** I share my screen. */
      sharingMe: computed(() => !!store.state()?.sharing.includes(user.user()?.pubkey ?? '')),
      /** The voice channel I'm in. */
      channel: computed(() => {
        const t = target();
        return t?.type === 'channel' ? t.channel : null;
      }),
      /** The call I'm in: its conversation (`roomKey`) and id. */
      call: computed(() => {
        const t = target();
        return t?.type === 'call' ? { key: roomKey(t.room), id: t.id } : null;
      }),
    };
  }),
  withMethods((store, shell = inject(ShellService)) => {
    let ringTimer: ReturnType<typeof setTimeout> | undefined;
    /** Draws the screen I watch (the stage), JPEG bytes. */
    let draw: ((jpeg: ArrayBuffer) => void) | null = null;
    /** The last join, for Retry. */
    let last: [string, Record<string, unknown>] | null = null;

    const stopRing = () => {
      clearTimeout(ringTimer);
      patchState(store, { ring: null });
    };

    /** Joins (or changes room); the state also comes as a `voice` event. */
    const join = async (command: string, args: Record<string, unknown>) => {
      last = [command, args];
      patchState(store, { busy: true, error: null });
      try {
        patchState(store, { state: await invoke<VoiceState>(command, args) });
      } catch (e) {
        patchState(store, { error: code(e) });
      } finally {
        patchState(store, { busy: false });
      }
    };

    const set = async (command: string, args: Record<string, unknown>) => {
      try {
        patchState(store, { state: await invoke<VoiceState>(command, args) });
      } catch (e) {
        patchState(store, { error: code(e) });
      }
    };

    /** From the picker: null shares what the OS asks for. */
    const pick = async (source: ScreenSource | null) => {
      patchState(store, { sources: null });
      await invoke('start_screen_share', { source }).catch((e: unknown) =>
        patchState(store, { error: code(e) }),
      );
    };

    return {
      /** Asks once per community whether its instance has voice. */
      async checkAvailable(community: string) {
        if (community in store.available()) {
          return;
        }
        const ok = await invoke<boolean>('voice_available', { community }).catch(() => false);
        patchState(store, (s) => ({ available: { ...s.available, [community]: ok } }));
      },

      joinChannel(community: string, channel: string) {
        return join('join_voice_channel', { community, channel });
      },

      /** Calls the conversation `room`. */
      startCall(room: string[]) {
        return join('start_call', { room });
      },

      /** Joins the call `id` of `room`, from its note or its ring. */
      joinCall(room: string[], id: string) {
        if (store.ring()?.message.call?.id === id) {
          stopRing();
        }
        return join('join_call', { room, id });
      },

      decline: stopRing,

      async leave() {
        patchState(store, { busy: true });
        await invoke('leave_voice').catch((e: unknown) => patchState(store, { error: code(e) }));
        patchState(store, { state: null, busy: false });
      },

      toggleMute() {
        const state = store.state();
        return state ? set('set_muted', { muted: !state.muted }) : undefined;
      },

      toggleDeafen() {
        const state = store.state();
        return state ? set('set_deafened', { deafened: !state.deafened }) : undefined;
      },

      /** After a reload: the room core is still in. */
      async sync() {
        const state = await invoke<VoiceState | null>('voice_state').catch(() => null);
        patchState(store, { state });
      },

      /** Shares a screen: the picker, or the OS's own when it has one. */
      async share() {
        const sources = await invoke<ScreenSource[]>('screen_sources').catch(() => []);
        if (sources.length) {
          patchState(store, { sources });
        } else {
          await pick(null);
        }
      },

      pick,

      closePicker() {
        patchState(store, { sources: null });
      },

      stopShare() {
        return invoke('stop_screen_share').catch(() => undefined);
      },

      /** Watches `key`'s screen; the stage shows it (on a phone, the sheet opens). */
      watch(key: string) {
        shell.openChat();
        const images = new Channel<ArrayBuffer>();
        images.onmessage = (jpeg) => draw?.(jpeg);
        return set('watch_screen', { key, images });
      },

      stopWatching() {
        return set('stop_watching', {});
      },

      /** The stage registers how to draw; null when it goes. */
      onImage(fn: ((jpeg: ArrayBuffer) => void) | null) {
        draw = fn;
      },

      clearError() {
        patchState(store, { error: null });
      },

      retry() {
        return last ? join(...last) : undefined;
      },

      onUpdate(update: VoiceUpdate) {
        if (update.type === 'voice') {
          patchState(store, update.state ? { state: update.state, error: null } : { state: null });
          return;
        }
        clearTimeout(ringTimer);
        patchState(store, { ring: { message: update.message, until: update.until } });
        ringTimer = setTimeout(stopRing, Math.max(0, update.until * 1000 - Date.now()));
      },

      /** The caller hung up before I answered. */
      onDm(message: DmMessage) {
        if (message.call?.ended && store.ring()?.message.call?.id === message.call.id) {
          stopRing();
        }
      },
    };
  }),
  withHooks({
    onInit(store) {
      void listen<VoiceUpdate>('voice', (event) => store.onUpdate(event.payload));
      void listen<DmMessage>('dm', (event) => store.onDm(event.payload));
      void store.sync();
      void invoke<boolean>('can_share_screen')
        .then((canShare) => patchState(store, { canShare }))
        .catch(() => undefined);
    },
  }),
);
