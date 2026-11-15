import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-chat-layout',
  imports: [],
  templateUrl: './chat.layout.html',
  styleUrl: './chat.layout.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChatLayout {}
