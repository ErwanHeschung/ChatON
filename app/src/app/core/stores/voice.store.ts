import { computed } from '@angular/core';
import {
  patchState,
  signalStore,
  withComputed,
  withHooks,
  withMethods,
  withState,
} from '@ngrx/signals';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CmdError } from '@models/Auth.model';
import { DmMessage, roomKey } from '@models/Dm.model';
import { VoiceState, VoiceUpdate } from '@models/Voice.model';

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
  }),
  withComputed((store) => {
    const target = computed(() => store.state()?.target);
    return {
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
  withMethods((store) => {
    let ringTimer: ReturnType<typeof setTimeout> | undefined;
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
    },
  }),
);
