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
import {
  ChannelMessage,
  Community,
  CommunityUpdate,
  Member,
  ROLE_ADMIN,
  ROLE_MODERATOR,
} from '@models/Community.model';
import { DmStore } from '@stores/dm.store';

/** My communities, the open channel and its members. Permissions are checked by the relay. */
export const CommunityStore = signalStore(
  { providedIn: 'root' },
  withState({
    communities: [] as Community[],
    selected: null as string | null,
    channel: null as string | null,
    messages: [] as ChannelMessage[],
    members: [] as Member[],
    error: null as CmdError['code'] | null,
  }),
  withComputed((store) => {
    const current = computed(() => store.communities().find((c) => c.id === store.selected()));
    return {
      current,
      isAdmin: computed(() => !!current()?.roles.includes(ROLE_ADMIN)),
      canModerate: computed(() =>
        current()?.roles.some((r) => r === ROLE_ADMIN || r === ROLE_MODERATOR),
      ),
    };
  }),
  withMethods((store, dm = inject(DmStore)) => {
    /** Runs a command, keeping its error code for the UI. */
    const run = async <T>(command: string, args?: Record<string, unknown>) => {
      try {
        const result = await invoke<T>(command, args);
        patchState(store, { error: null });
        return { ok: true as const, result };
      } catch (e) {
        patchState(store, { error: (e as CmdError).code ?? 'internal' });
        return { ok: false as const };
      }
    };

    const load = async () => {
      const communities = await invoke<Community[]>('communities');
      patchState(store, { communities });
      if (store.selected() && !store.current()) {
        // Left, or removed by a moderator.
        patchState(store, { selected: null, channel: null, messages: [], members: [] });
      }
    };

    const loadMembers = async () => {
      const community = store.selected();
      if (!community) {
        return;
      }
      const members = await invoke<Member[]>('members', { community });
      if (store.selected() === community) {
        patchState(store, { members });
        members.forEach((m) => void dm.resolve(m.pubkey));
      }
    };

    const selectChannel = async (channel: string | null) => {
      patchState(store, { channel, messages: [] });
      if (!channel) {
        return;
      }
      const messages = await invoke<ChannelMessage[]>('channel_messages', { channel });
      if (store.channel() === channel) {
        patchState(store, { messages });
        new Set(messages.map((m) => m.author)).forEach((pk) => void dm.resolve(pk));
      }
    };

    /** Opens a community on its first channel (`null`: the first one I have). */
    const select = async (id: string | null = store.selected() ?? store.communities()[0]?.id) => {
      patchState(store, { selected: id ?? null, members: [], error: null });
      await selectChannel(store.current()?.channels[0]?.id ?? null);
      await loadMembers();
    };

    const receive = (message: ChannelMessage) => {
      patchState(store, (s) => ({
        messages:
          s.channel === message.channel && !s.messages.some((m) => m.id === message.id)
            ? [...s.messages, message]
            : s.messages,
      }));
      void dm.resolve(message.author);
    };

    const forget = (ids: string[]) =>
      patchState(store, (s) => ({ messages: s.messages.filter((m) => !ids.includes(m.id)) }));

    const joined = async (community: Community) => {
      await load();
      await select(community.id);
    };

    return {
      load,
      select,
      selectChannel,

      onUpdate(update: CommunityUpdate) {
        switch (update.type) {
          case 'channel_message':
            return receive(update.message);
          case 'deleted':
            return forget(update.ids);
          case 'communities':
            return void load().then(loadMembers);
        }
      },

      async create(name: string) {
        const res = await run<Community>('create_community', { name });
        if (res.ok) await joined(res.result);
        return res.ok;
      },

      async join(invite: string) {
        const res = await run<Community>('join_community', { invite });
        if (res.ok) await joined(res.result);
        return res.ok;
      },

      async leave() {
        const community = store.selected();
        if (community && (await run('leave_community', { community })).ok) {
          await load();
        }
      },

      async createChannel(name: string) {
        const community = store.selected();
        if (community && name.trim() && (await run('create_channel', { community, name })).ok) {
          await load();
        }
      },

      async invite() {
        const res = await run<string>('create_invite', { community: store.selected() });
        return res.ok ? res.result : null;
      },

      async send(text: string) {
        const [community, channel] = [store.selected(), store.channel()];
        if (!community || !channel || !text.trim()) {
          return;
        }
        const res = await run<ChannelMessage>('send_channel', { community, channel, text });
        if (res.ok) receive(res.result);
      },

      async remove(id: string) {
        const [community, channel] = [store.selected(), store.channel()];
        if (community && channel && (await run('delete_message', { community, channel, id })).ok) {
          forget([id]);
        }
      },

      async setRoles(pubkey: string, roles: string[]) {
        if ((await run('set_roles', { community: store.selected(), pubkey, roles })).ok) {
          await loadMembers();
        }
      },

      async removeMember(pubkey: string) {
        if ((await run('remove_member', { community: store.selected(), pubkey })).ok) {
          await loadMembers();
        }
      },
    };
  }),
  withHooks({
    onInit(store) {
      void listen<CommunityUpdate>('community', (event) => store.onUpdate(event.payload));
    },
  }),
);
