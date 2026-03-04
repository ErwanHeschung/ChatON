import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { Chat } from '@components/chats/chat/chat';
import { HorizontalList } from '@components/horizontal-list/horizontal-list';
import { UsersList } from '@components/users/users-list/users-list';
import { ChatLayout } from '@layouts/chat/chat.layout';

@Component({
  selector: 'app-root-chat',
  imports: [ChatLayout, HorizontalList, RouterOutlet, UsersList, Chat],
  templateUrl: './root-chat.page.html',
  styleUrl: './root-chat.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RootChat {}
