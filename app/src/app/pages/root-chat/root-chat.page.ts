import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Chat } from '@components/chats/chat/chat';
import { HorizontalList } from '@components/horizontal-list/horizontal-list';
import { UsersList } from '@components/users/users-list/users-list';
import { ChatLayout } from '@layouts/chat/chat.layout';
import { Attachment } from '@models/Media.model';
import { Message, MessageStatus } from '@models/Message.model';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** The chat shows the open channel in the server view, else the open DM. */
@Component({
  selector: 'app-root-chat',
  imports: [ChatLayout, HorizontalList, RouterOutlet, UsersList, Chat],
  templateUrl: './root-chat.page.html',
  styleUrl: './root-chat.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RootChat {
  public readonly dm = inject(DmStore);
  public readonly community = inject(CommunityStore);
  private readonly user = inject(ConnectedUserStore);

  public readonly inChannel = computed(() => this.community.channel() !== null);

  // Newest first for the bottom-up chat list.
  public readonly messages = computed<Message[]>(() => {
    if (!this.inChannel()) {
      return [...this.dm.messages()].reverse().map((m) => ({
        ...this.message(m.id, m.sender, m.text, m.created_at, m.attachment),
        subject: m.subject,
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
      text,
      timestamp: new Date(createdAt * 1000),
      status: MessageStatus.SENT,
      attachment,
    };
  }
}
