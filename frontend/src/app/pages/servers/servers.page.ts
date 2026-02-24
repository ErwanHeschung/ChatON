import { ChangeDetectionStrategy, Component } from '@angular/core';
import { ChatLayout } from '@layouts/chat/chat.layout';
import { HorizontalList } from '@components/horizontal-list/horizontal-list';
import { UsersList } from '@components/users/users-list/users-list';
import { Chat } from '@components/chats/chat/chat';

@Component({
  selector: 'app-servers.page',
  imports: [ChatLayout, HorizontalList, UsersList, Chat],
  templateUrl: './servers.page.html',
  styleUrl: './servers.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServersPage {}
