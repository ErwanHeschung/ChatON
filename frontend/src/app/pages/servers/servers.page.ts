import { Component } from '@angular/core';
import { ChatLayout } from '../../shared/layouts/chat/chat.layout';
import { HorizontalList } from '../../shared/components/horizontal-list/horizontal-list';

@Component({
  selector: 'app-servers.page',
  imports: [ChatLayout, HorizontalList],
  templateUrl: './servers.page.html',
  styleUrl: './servers.page.scss',
})
export class ServersPage {}
