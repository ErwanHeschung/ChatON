import { inject } from '@angular/core';
import { patchState, signalStore, withHooks, withMethods, withState } from '@ngrx/signals';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CmdError } from '@models/Auth.model';
import { Conversation, DmMessage, Peer, peerLabel } from '@models/Dm.model';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { MediaService } from '@services/media.service';

/** Private conversations. Everything comes from core; nothing is decrypted here. */
export const DmStore = signalStore(
  { providedIn: 'root' },
  withState({
    conversations: [] as Conversation[],
    peers: {} as Record<string, Peer>,
    selected: null as string | null,
    messages: [] as DmMessage[],
    searchError: null as CmdError['code'] | null,
    sendError: null as CmdError['code'] | null,
  }),
  withMethods((store, user = inject(ConnectedUserStore), media = inject(MediaService)) => {
    const resolvePeer = async (pubkey: string, refresh = false) => {
      if (store.peers()[pubkey] && !refresh) {
        return;
      }
      const peer = await invoke<Peer>('peer', { pubkey });
      patchState(store, (s) => ({ peers: { ...s.peers, [pubkey]: peer } }));
    };

    const select = async (pubkey: string | null) => {
      patchState(store, { selected: pubkey, messages: [], sendError: null });
      if (!pubkey) {
        return;
      }
      const messages = await invoke<DmMessage[]>('messages', { pubkey });
      if (store.selected() === pubkey) {
        patchState(store, { messages });
      }
    };

    // Same path for incoming (`dm` event) and sent messages.
    const receive = (message: DmMessage) => {
      const me = user.user()?.pubkey;
      const peer = message.sender === me ? message.receiver : message.sender;
      patchState(store, (s) => ({
        conversations: [{ peer, last: message }, ...s.conversations.filter((c) => c.peer !== peer)],
        messages:
          s.selected === peer && !s.messages.some((m) => m.id === message.id)
            ? [...s.messages, message]
            : s.messages,
      }));
      void resolvePeer(peer);
    };

    return {
      select,
      receive,
      /** Resolves a key's handle once, for `label` (also used for community members). */
      resolve: resolvePeer,
      label: (pubkey: string) => peerLabel(store.peers()[pubkey], pubkey),

      async load() {
        const conversations = await invoke<Conversation[]>('conversations');
        patchState(store, { conversations });
        await Promise.all(conversations.map((c) => resolvePeer(c.peer)));
      },

      async search(handle: string) {
        try {
          const peer = await invoke<Peer>('search_handle', { handle });
          patchState(store, (s) => ({
            peers: { ...s.peers, [peer.pubkey]: peer },
            searchError: null,
          }));
          await select(peer.pubkey);
        } catch (e) {
          patchState(store, { searchError: (e as CmdError).code ?? 'internal' });
        }
      },

      async send(text: string) {
        const pubkey = store.selected();
        if (!pubkey || !text.trim()) {
          return;
        }
        try {
          receive(await invoke<DmMessage>('send_dm', { pubkey, text }));
          patchState(store, { sendError: null });
        } catch (e) {
          patchState(store, { sendError: (e as CmdError).code ?? 'internal' });
        }
      },

      async sendFile(file: File) {
        const pubkey = store.selected();
        if (!pubkey) {
          return;
        }
        try {
          receive(await media.upload<DmMessage>('send_dm_file', file, { pubkey }));
          patchState(store, { sendError: null });
        } catch (e) {
          patchState(store, { sendError: (e as CmdError).code ?? 'internal' });
        }
      },
    };
  }),
  withHooks({
    onInit(store) {
      void listen<DmMessage>('dm', (event) => store.receive(event.payload));
    },
  }),
);
