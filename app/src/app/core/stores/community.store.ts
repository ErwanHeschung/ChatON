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
import { VoiceAction } from '@models/Voice.model';
import {
  AuditEntry,
  Ban,
  Channel,
  ChannelEdit,
  ChannelMessage,
  Community,
  InviteInfo,
  CommunityEdit,
  CommunityUpdate,
  Member,
  Moderation,
  Permission,
} from '@models/Community.model';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { MediaService } from '@services/media.service';

/** My communities, the open channel and its members. Permissions are checked by the relay. */
export const CommunityStore = signalStore(
  { providedIn: 'root' },
  withState({
    communities: [] as Community[],
    selected: null as string | null,
    channel: null as string | null,
    messages: [] as ChannelMessage[],
    /** The message my next one answers (spec § 9, Messages). */
    replyTo: null as string | null,
    members: [] as Member[],
    error: null as CmdError['code'] | null,
    sendError: null as CmdError['code'] | null,
  }),
  withComputed((store, user = inject(ConnectedUserStore)) => {
    const current = computed(() => store.communities().find((c) => c.id === store.selected()));
    return {
      current,
      isOwner: computed(() => !!current()?.owner && current()?.owner === user.user()?.pubkey),
    };
  }),
  withMethods((store, dm = inject(DmStore), media = inject(MediaService)) => {
    /** I have `perm` in the open community (computed by core, checked again by the relay). */
    const can = (perm: Permission) => !!store.current()?.perms.includes(perm);

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
      patchState(store, { channel, messages: [], replyTo: null });
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

    /** Reactions or pins changed: the open channel, read again in place. */
    const refresh = async (channel: string) => {
      if (store.channel() !== channel) return;
      const messages = await invoke<ChannelMessage[]>('channel_messages', { channel });
      if (store.channel() === channel) patchState(store, { messages });
    };

    const forget = (ids: string[]) =>
      patchState(store, (s) => ({ messages: s.messages.filter((m) => !ids.includes(m.id)) }));

    const removeMember = async (pubkey: string, reason = '') => {
      const ok = (await run('remove_member', { community: store.selected(), pubkey, reason })).ok;
      if (ok) await loadMembers();
      return ok;
    };

    const joined = async (community: Community) => {
      await load();
      await select(community.id);
    };

    return {
      can,
      /** I have `perm` in this channel of the open community. */
      canIn: (channel: Channel, perm: Permission) => channel.perms.includes(perm),
      load,
      select,
      selectChannel,

      onUpdate(update: CommunityUpdate) {
        switch (update.type) {
          case 'channel_message':
            return receive(update.message);
          case 'deleted':
            return forget(update.ids);
          case 'channel_changed':
            return void refresh(update.channel);
          case 'communities':
            return void load().then(loadMembers);
          case 'blocks':
            return void selectChannel(store.channel());
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

      /** manage_channels: at the end of the community, or of `category`. */
      async createChannel(name: string, voice = false, category: string | null = null) {
        const community = store.selected();
        const ok =
          !!community &&
          !!name.trim() &&
          (await run('create_channel', { community, name: name.trim(), voice, category })).ok;
        if (ok) await load();
        return ok;
      },

      /** manage_channels: channels in this order and these categories, shown at once. */
      async arrange(channels: { id: string; category: string | null }[]) {
        const id = store.selected();
        patchState(store, (s) => ({
          communities: s.communities.map((c) =>
            c.id !== id
              ? c
              : {
                  ...c,
                  channels: channels.flatMap((p) => {
                    const ch = c.channels.find((x) => x.id === p.id);
                    return ch ? [{ ...ch, category: p.category }] : [];
                  }),
                },
          ),
        }));
        const ok = (await run('update_community', { community: id, edit: { channels } })).ok;
        await load(); // the relay's order, or back to it if refused
        return ok;
      },

      /** manage_channels in it: name, topic, slow mode, user limit. */
      async editChannel(channel: string, edit: ChannelEdit) {
        const ok = (await run('edit_channel', { community: store.selected(), channel, edit })).ok;
        if (ok) await load();
        return ok;
      },

      async deleteChannel(channel: string) {
        const ok = (await run('delete_channel', { community: store.selected(), channel })).ok;
        if (ok) await load();
        return ok;
      },

      /** A new invite: stops at `expires` (unix seconds) or after `maxUses` joins; 0: no limit. */
      async invite(expires = 0, maxUses = 0) {
        const res = await run<string>('create_invite', {
          community: store.selected(),
          expires,
          maxUses,
        });
        return res.ok ? res.result : null;
      },

      async invites() {
        const r = await run<InviteInfo[]>('invites', { community: store.selected() });
        if (r.ok) r.result.forEach((i) => void dm.resolve(i.author));
        return r.ok ? r.result : [];
      },

      async revokeInvite(code: string) {
        return (await run('revoke_invite', { community: store.selected(), code })).ok;
      },

      async send(text: string) {
        const [community, channel] = [store.selected(), store.channel()];
        if (!community || !channel || !text.trim()) {
          return;
        }
        const parent = store.replyTo();
        const res = parent
          ? await run<ChannelMessage>('reply_channel', { community, channel, text, parent })
          : await run<ChannelMessage>('send_channel', { community, channel, text });
        patchState(store, { sendError: res.ok ? null : store.error() });
        if (res.ok) patchState(store, { replyTo: null });
        if (res.ok) receive(res.result);
      },

      async sendFile(file: File) {
        const [community, channel] = [store.selected(), store.channel()];
        if (!community || !channel) {
          return;
        }
        try {
          receive(
            await media.upload<ChannelMessage>('send_channel_file', file, { community, channel }),
          );
          patchState(store, { sendError: null });
        } catch (e) {
          patchState(store, { sendError: (e as CmdError).code ?? 'internal' });
        }
      },

      /** manage_community: new picture or banner (public, on my instance). */
      async setPicture(file: File, banner = false) {
        const community = store.selected();
        if (!community) {
          return false;
        }
        try {
          await media.upload('set_community_picture', file, {
            community,
            ...(banner ? { banner: '1' } : {}),
          });
          patchState(store, { error: null });
          await load();
          return true;
        } catch (e) {
          patchState(store, { error: (e as CmdError).code ?? 'internal' });
          return false;
        }
      },

      /** Name, description, roles or owner: core sends the whole settings (spec § 9). */
      async update(edit: CommunityEdit) {
        const res = await run('update_community', { community: store.selected(), edit });
        if (res.ok) {
          await load();
          await loadMembers();
        }
        return res.ok;
      },

      /** Owner: deletes it for everyone. */
      async deleteCommunity() {
        const community = store.selected();
        if (community && (await run('delete_community', { community })).ok) {
          await load();
          return true;
        }
        return false;
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

      removeMember,

      /** Bans or times out (`until`, unix seconds; 0 lifts it) a member (spec § 9, Modération). */
      async moderate(action: Moderation, pubkey: string, reason: string, until = 0) {
        if (action === 'kick') return removeMember(pubkey, reason);
        const community = store.selected();
        const ok = (
          action === 'ban'
            ? await run('ban', { community, pubkey, reason })
            : await run('timeout', { community, pubkey, until, reason })
        ).ok;
        if (ok) await loadMembers();
        return ok;
      },

      /** The message the next one answers; null: none. */
      replyingTo(id: string | null) {
        patchState(store, { replyTo: id });
      },

      /** Adds (`on`) or takes back my `emoji` on message `id` of the open channel. */
      async react(id: string, emoji: string, on: boolean) {
        const [community, channel] = [store.selected(), store.channel()];
        if ((await run('react', { community, channel, id, emoji, on })).ok && channel) {
          await refresh(channel);
        }
      },

      async setPinned(id: string, pinned: boolean) {
        const [community, channel] = [store.selected(), store.channel()];
        if ((await run('set_pinned', { community, channel, id, pinned })).ok && channel) {
          await refresh(channel);
        }
      },

      /** In voice `channel` (spec § 11): mute, deafen, disconnect, or move `target` to `to`. */
      async moderateVoice(channel: string, target: string, action: VoiceAction, to?: string) {
        const community = store.selected();
        return (await run('moderate_voice', { community, channel, target, action, to })).ok;
      },

      async unban(pubkey: string) {
        return (await run('unban', { community: store.selected(), pubkey })).ok;
      },

      async bans() {
        const r = await run<Ban[]>('bans', { community: store.selected() });
        if (r.ok) r.result.forEach((b) => void dm.resolve(b.pubkey));
        return r.ok ? r.result : [];
      },

      async auditLog() {
        const r = await run<AuditEntry[]>('audit_log', { community: store.selected() });
        if (r.ok) {
          new Set(r.result.flatMap((e) => [e.actor, e.target ?? []].flat())).forEach(
            (pk) => void dm.resolve(pk),
          );
        }
        return r.ok ? r.result : [];
      },
    };
  }),
  withHooks({
    onInit(store) {
      void listen<CommunityUpdate>('community', (event) => store.onUpdate(event.payload));
    },
  }),
);
