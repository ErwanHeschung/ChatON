import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Chat } from '@components/chats/chat/chat';
import { ChatHeader } from '@components/chats/chat-header/chat-header';
import { RequestBanner } from '@components/chats/request-banner/request-banner';
import { Network } from '@components/network/network';
import { CommunityRail } from '@components/community-rail/community-rail';
import { UserPanel } from '@components/users/user-panel/user-panel';
import { UsersList } from '@components/users/users-list/users-list';
import { Settings } from '@components/settings/settings';
import { CallBanner } from '@components/voice/call-banner/call-banner';
import { ScreenStage } from '@components/voice/screen-stage/screen-stage';
import { VoiceBar } from '@components/voice/voice-bar/voice-bar';
import { ShellService } from '@services/shell.service';
import { ChatLayout } from '@layouts/chat/chat.layout';
import { Attachment } from '@models/Media.model';
import { Message, MessageStatus } from '@models/Message.model';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/** The chat shows the open channel in the server view, else the open DM. */
@Component({
  selector: 'app-root-chat',
  imports: [
    ChatLayout,
    CommunityRail,
    ChatHeader,
    RouterOutlet,
    UsersList,
    UserPanel,
    Chat,
    Settings,
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
    const moderator = this.community.canModerate();
    return [...this.community.messages()].reverse().map((m) => ({
      ...this.message(m.id, m.author, m.text, m.created_at, m.attachment),
      deletable: moderator || m.author === me,
    }));
  });

  public send(text: string): void {
    void (this.inChannel() ? this.community.send(text) : this.dm.send(text));
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
