import { computed, inject } from '@angular/core';
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
import { Conversation, DmMessage, Peer, peerLabel, roomKey } from '@models/Dm.model';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { MediaService } from '@services/media.service';

const code = (e: unknown): CmdError['code'] => (e as CmdError).code ?? 'internal';

/** Private conversations, 1:1 and groups. Everything comes from core; nothing is decrypted here. */
export const DmStore = signalStore(
  { providedIn: 'root' },
  withState({
    conversations: [] as Conversation[],
    peers: {} as Record<string, Peer>,
    /** Room key (`roomKey`) of the open conversation. */
    selected: null as string | null,
    messages: [] as DmMessage[],
    searchError: null as CmdError['code'] | null,
    sendError: null as CmdError['code'] | null,
    /** Members already in the group that "Add people" starts from. */
    groupDraft: [] as string[],
  }),
  withComputed((store) => {
    const room = computed(() => store.selected()?.split(',') ?? []);
    return { room, isGroup: computed(() => room().length > 2) };
  }),
  withMethods((store, user = inject(ConnectedUserStore), media = inject(MediaService)) => {
    const resolvePeer = async (pubkey: string, refresh = false) => {
      if (store.peers()[pubkey] && !refresh) {
        return;
      }
      const peer = await invoke<Peer>('peer', { pubkey });
      patchState(store, (s) => ({ peers: { ...s.peers, [pubkey]: peer } }));
    };
    const label = (pubkey: string) => peerLabel(store.peers()[pubkey], pubkey);
    const others = (key: string) => key.split(',').filter((pk) => pk !== user.user()?.pubkey);

    /** Messages of `key`, without clearing what's shown (after a send that partly failed). */
    const reload = async (key: string) => {
      const messages = await invoke<DmMessage[]>('messages', { room: key.split(',') });
      if (store.selected() === key) {
        patchState(store, { messages });
      }
    };

    const select = async (key: string | null) => {
      patchState(store, { selected: key, messages: [], sendError: null });
      if (key) {
        await reload(key);
      }
    };

    // Same path for incoming (`dm` event) and sent messages.
    const receive = (message: DmMessage) => {
      const key = roomKey(message.room);
      patchState(store, (s) => {
        const old = s.conversations.find((c) => roomKey(c.room) === key);
        const conversation = {
          room: message.room,
          subject: message.subject ?? old?.subject ?? null,
          last: message,
        };
        return {
          conversations: [conversation, ...s.conversations.filter((c) => c !== old)],
          messages:
            s.selected === key && !s.messages.some((m) => m.id === message.id)
              ? [...s.messages, message]
              : s.messages,
        };
      });
      message.room.forEach((pk) => void resolvePeer(pk));
    };

    /** Sends in the open conversation. Partly delivered: core kept my copy, show it. */
    const deliver = async (send: (room: string[]) => Promise<DmMessage>) => {
      const key = store.selected();
      if (!key) {
        return;
      }
      try {
        receive(await send(key.split(',')));
        patchState(store, { sendError: null });
      } catch (e) {
        patchState(store, { sendError: code(e) });
        void reload(key);
      }
    };

    return {
      select,
      receive,
      /** Resolves a key's handle once, for `label` (also used for community members). */
      resolve: resolvePeer,
      label,
      others,

      /** The group's name, else its other members' handles. */
      title(key: string) {
        const subject = store.conversations().find((c) => roomKey(c.room) === key)?.subject;
        return subject ?? others(key).map(label).join(', ');
      },

      async load() {
        const conversations = await invoke<Conversation[]>('conversations');
        patchState(store, { conversations });
        const keys = new Set(conversations.flatMap((c) => c.room));
        await Promise.all([...keys].map((pk) => resolvePeer(pk)));
      },

      /** Finds `@pseudo@instance` and opens the 1:1 with them. */
      async search(handle: string) {
        try {
          const peer = await invoke<Peer>('search_handle', { handle });
          patchState(store, (s) => ({
            peers: { ...s.peers, [peer.pubkey]: peer },
            searchError: null,
          }));
          // Core sorts rooms by key bytes: the same order as lowercase hex strings.
          await select(roomKey([user.user()?.pubkey ?? '', peer.pubkey].sort()));
        } catch (e) {
          patchState(store, { searchError: code(e) });
        }
      },

      async send(text: string) {
        if (text.trim()) {
          await deliver((room) => invoke<DmMessage>('send_dm', { room, text }));
        }
      },

      async sendFile(file: File) {
        await deliver((room) =>
          media.upload<DmMessage>('send_dm_file', file, { room: room.join(',') }),
        );
      },

      /** Creates a group (its first event is its name) and opens it. Null when done. */
      async createGroup(members: string[], subject: string): Promise<CmdError['code'] | null> {
        try {
          const message = await invoke<DmMessage>('name_group', { room: members, subject });
          receive(message);
          await select(roomKey(message.room));
          return null;
        } catch (e) {
          return code(e);
        }
      },

      async rename(subject: string) {
        if (subject.trim()) {
          await deliver((room) => invoke<DmMessage>('name_group', { room, subject }));
        }
      },

      setGroupDraft(members: string[]) {
        patchState(store, { groupDraft: members });
      },
    };
  }),
  withHooks({
    onInit(store) {
      void listen<DmMessage>('dm', (event) => store.receive(event.payload));
    },
  }),
);
