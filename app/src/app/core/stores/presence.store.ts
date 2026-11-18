import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Presence, PresenceUpdate, Status } from '@models/Presence.model';

/** No input for this long: online shows away (spec § 12). */
const IDLE_MS = 10 * 60 * 1000;

/** Who is online, and my own status. Core sends and reads the signals. */
export const PresenceStore = signalStore(
  { providedIn: 'root' },
  withState({
    online: {} as Record<string, Presence>,
    mine: 'online' as Status,
    me: '',
  }),
  withMethods((store) => ({
    /** What I see of `pubkey`; for me, the status I chose. */
    of(pubkey: string): Status | 'offline' {
      return pubkey === store.me() ? store.mine() : (store.online()[pubkey] ?? 'offline');
    },
    /** Signed in as `me`: read my status. */
    async load(me: string): Promise<void> {
      patchState(store, { me, mine: await invoke<Status>('my_status') });
    },
    async set(status: Status): Promise<void> {
      const before = store.mine();
      patchState(store, { mine: status });
      try {
        await invoke('set_status', { status });
      } catch {
        patchState(store, { mine: before });
      }
    },
  })),
  withHooks({
    onInit(store) {
      void listen<PresenceUpdate>('presence', ({ payload }) => {
        if (payload.type === 'presence') patchState(store, { online: payload.online });
        else patchState(store, { mine: payload.status });
      });
      // Away after IDLE_MS without input, or hidden (background, minimized); back on input.
      let last = Date.now();
      let away = false;
      const setAway = (value: boolean): void => {
        if (away === value || !store.me()) return;
        away = value;
        invoke('set_away', { away: value }).catch(() => (away = !value));
      };
      const active = (): void => {
        last = Date.now();
        if (!document.hidden) setAway(false);
      };
      for (const type of ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart']) {
        window.addEventListener(type, active, { passive: true });
      }
      document.addEventListener('visibilitychange', () =>
        document.hidden ? setAway(true) : active(),
      );
      setInterval(() => {
        if (Date.now() - last > IDLE_MS) setAway(true);
      }, 30_000);
    },
  }),
);
