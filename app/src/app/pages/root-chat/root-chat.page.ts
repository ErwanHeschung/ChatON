import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { LucideClock, LucideDynamicIcon } from '@lucide/angular';
import { DatePipe } from '@angular/common';
import { RouterOutlet } from '@angular/router';
import { Chat } from '@components/chats/chat/chat';
import { ChatHeader } from '@components/chats/chat-header/chat-header';
import { RequestBanner } from '@components/chats/request-banner/request-banner';
import { Network } from '@components/network/network';
import { CommunityRail } from '@components/community-rail/community-rail';
import { UserPanel } from '@components/users/user-panel/user-panel';
import { UsersList } from '@components/users/users-list/users-list';
import { Settings } from '@components/settings/settings';
import { ServerSettings } from '@components/servers/server-settings/server-settings';
import { CallBanner } from '@components/voice/call-banner/call-banner';
import { ScreenStage } from '@components/voice/screen-stage/screen-stage';
import { VoiceBar } from '@components/voice/voice-bar/voice-bar';
import { ShellService } from '@services/shell.service';
import { ChatLayout } from '@layouts/chat/chat.layout';
import { Attachment } from '@models/Media.model';
import { EVERYONE, Permission } from '@models/Community.model';
import { handleText } from '@models/Dm.model';
import { MentionCandidate, Message, MessageStatus } from '@models/Message.model';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/** The chat shows the open channel in the server view, else the open DM. */
@Component({
  selector: 'app-root-chat',
  imports: [
    DatePipe,
    LucideDynamicIcon,
    ChatLayout,
    CommunityRail,
    ChatHeader,
    RouterOutlet,
    UsersList,
    UserPanel,
    Chat,
    Settings,
    ServerSettings,
    VoiceBar,
    CallBanner,
    ScreenStage,
    RequestBanner,
    Network,
  ],
  templateUrl: './root-chat.page.html',
  styleUrl: './root-chat.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RootChat {
  public readonly dm = inject(DmStore);
  protected readonly shell = inject(ShellService);
  public readonly community = inject(CommunityStore);
  private readonly user = inject(ConnectedUserStore);
  protected readonly voice = inject(VoiceStore);

  public readonly inChannel = computed(() => this.community.channel() !== null);

  /** The end of my timeout in the open community, while it runs (spec § 9, Modération). */
  private readonly now = signal(Date.now());
  protected readonly timedOut = computed(() => {
    const until = (this.community.current()?.timeout ?? 0) * 1000;
    return this.inChannel() && until > this.now() ? until : 0;
  });
  protected readonly clock = LucideClock;

  constructor() {
    // Lift the banner when the timeout ends: the relay doesn't announce it.
    effect((cleanup) => {
      const until = this.timedOut();
      if (!until) return;
      const timer = setTimeout(() => this.now.set(Date.now()), until - Date.now() + 500);
      cleanup(() => clearTimeout(timer));
    });
  }

  // Newest first for the bottom-up chat list.
  public readonly messages = computed<Message[]>(() => {
    if (!this.inChannel()) {
      const messages = this.dm.messages();
      // A call can be joined until its end arrives, unless I'm in it.
      const ended = new Set(messages.filter((m) => m.call?.ended).map((m) => m.call?.id));
      const current = this.voice.call()?.id;
      return [...messages].reverse().map((m) => ({
        ...this.message(m.id, m.sender, m.text, m.created_at, m.attachment),
        subject: m.subject,
        call: m.call && {
          ...m.call,
          joinable: !m.call.ended && !ended.has(m.call.id) && current !== m.call.id,
        },
      }));
    }
    const me = this.user.user()?.pubkey;
    const moderator = this.community.can('manage_messages');
    const all = this.community.messages();
    const byId = new Map(all.map((m) => [m.id, m]));
    const channel = this.community
      .current()
      ?.channels.find((c) => c.id === this.community.channel());
    const has = (p: Permission) =>
      !!channel && this.community.canIn(channel, p) && !this.timedOut();
    const roles = this.community.current()?.all_roles ?? [];
    // What a mention looks like in the text: the handle, the role's name, @everyone.
    const mentionText = (target: string) =>
      target === EVERYONE
        ? '@everyone'
        : /^[0-9a-f]{64}$/.test(target)
          ? handleText(this.dm.label(target))
          : '@' + (roles.find((r) => r.id === target)?.name ?? target);
    const can = {
      reply: has('send_messages'),
      react: has('add_reactions'),
      pin: has('manage_messages'),
    };
    return [...all].reverse().map((m) => {
      const parent = m.reply_to && byId.get(m.reply_to.id);
      return {
        ...this.message(m.id, m.author, m.text, m.created_at, m.attachment),
        deletable: moderator || m.author === me,
        reply: m.reply_to && {
          name: this.dm.label(m.reply_to.author),
          text: parent ? parent.text || parent.attachment?.name || '' : null,
        },
        reactions: m.reactions,
        pinned: m.pinned,
        mentions: m.mentions.map(mentionText),
        mentionsMe: m.mention,
        can,
      };
    });
  });

  /** Whom my next message answers. */
  protected readonly replying = computed(() => {
    const id = this.community.replyTo();
    const parent = id && this.community.messages().find((m) => m.id === id);
    return parent ? this.dm.label(parent.author) : null;
  });

  /** Who `@` may mention in the open channel: members, roles I may ping, @everyone. */
  protected readonly candidates = computed<MentionCandidate[]>(() => {
    const channel = this.community
      .current()
      ?.channels.find((c) => c.id === this.community.channel());
    if (!this.inChannel() || !channel) return [];
    const me = this.user.user()?.pubkey;
    const all = this.community.canIn(channel, 'mention_everyone');
    const people = this.community
      .members()
      .filter((m) => m.pubkey !== me)
      .map((m): MentionCandidate => {
        const label = this.dm.label(m.pubkey);
        const peer = this.dm.peers()[m.pubkey];
        const text = handleText(label);
        return {
          id: m.pubkey,
          kind: 'person',
          name: peer?.display_name ?? text,
          text,
          picture: peer?.picture,
        };
      });
    const roles = (this.community.current()?.all_roles ?? [])
      .filter((r) => r.id !== EVERYONE && (all || r.attrs.includes('mentionable')))
      .map((r): MentionCandidate => ({
        id: r.id,
        kind: 'role',
        name: r.name,
        text: `@${r.name}`,
        color: r.color,
      }));
    const everyone: MentionCandidate = {
      id: EVERYONE,
      kind: 'everyone',
      name: '@everyone',
      text: '@everyone',
    };
    return [...people, ...roles, ...(all ? [everyone] : [])];
  });

  public send({ text, mentions }: { text: string; mentions: string[] }): void {
    void (this.inChannel() ? this.community.send(text, mentions) : this.dm.send(text));
  }

  public sendFile(file: File): void {
    void (this.inChannel() ? this.community.sendFile(file) : this.dm.sendFile(file));
  }

  /** From a call note: joins that call of the open conversation. */
  public joinCall(message: Message): void {
    const room = this.dm.room();
    if (message.call && room.length) void this.voice.joinCall(room, message.call.id);
  }

  public remove(id: string): void {
    void this.community.remove(id);
  }

  private message(
    id: string,
    sender: string,
    text: string,
    createdAt: number,
    attachment: Attachment | null,
  ): Message {
    return {
      id,
      senderId: sender,
      senderName: this.dm.label(sender),
      senderPicture: this.dm.peers()[sender]?.picture,
      mine: sender === this.user.user()?.pubkey,
      text,
      timestamp: new Date(createdAt * 1000),
      status: MessageStatus.SENT,
      attachment,
    };
  }
}
