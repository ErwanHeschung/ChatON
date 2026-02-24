import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { Message } from '@models/Message.model';
import { NgOptimizedImage } from '@angular/common';

@Component({
  selector: 'app-chat-bubble',
  imports: [NgOptimizedImage],
  templateUrl: './chat-bubble.html',
  styleUrl: './chat-bubble.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChatBubble {
  public message = input.required<Message>();
}
