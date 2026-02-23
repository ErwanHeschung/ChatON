import { Component } from '@angular/core';
import { ChatLayout } from '../../shared/layouts/chat/chat.layout';
import { HorizontalList } from '../../shared/components/horizontal-list/horizontal-list';
import { UsersList } from '../../shared/components/users-list/users-list';
import { Chat } from '../../shared/components/chat/chat';

@Component({
  selector: 'app-servers.page',
  imports: [ChatLayout, HorizontalList, UsersList, Chat],
  templateUrl: './servers.page.html',
  styleUrl: './servers.page.scss',
})
export class ServersPage {}
