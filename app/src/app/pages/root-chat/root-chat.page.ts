import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Chat } from '@components/chats/chat/chat';
import { HorizontalList } from '@components/horizontal-list/horizontal-list';
import { UsersList } from '@components/users/users-list/users-list';
import { ChatLayout } from '@layouts/chat/chat.layout';
import { Message, MessageStatus } from '@models/Message.model';
import { DmStore } from '@stores/dm.store';

@Component({
  selector: 'app-root-chat',
  imports: [ChatLayout, HorizontalList, RouterOutlet, UsersList, Chat],
  templateUrl: './root-chat.page.html',
  styleUrl: './root-chat.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RootChat {
  public readonly dm = inject(DmStore);

  // Newest first for the bottom-up chat list.
  public readonly messages = computed<Message[]>(() =>
    [...this.dm.messages()].reverse().map((m) => ({
      id: m.id,
      senderId: m.sender,
      senderName: this.dm.label(m.sender),
      text: m.text,
      timestamp: new Date(m.created_at * 1000),
      status: MessageStatus.SENT,
    })),
  );
}
