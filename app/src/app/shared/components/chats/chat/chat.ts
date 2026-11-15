import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';
import { Message } from '@models/Message.model';
import { ChatBubble } from '@components/chats/chat-bubble/chat-bubble';
import { ChatBar } from '@components/chats/chat-bar/chat-bar';

@Component({
  selector: 'app-chat',
  imports: [ChatBubble, ChatBar],
  templateUrl: './chat.html',
  styleUrl: './chat.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Chat {
  /** Newest first: the list is rendered bottom-up (`column-reverse`). */
  public readonly messages = input<Message[]>([]);
  public readonly disabled = input(false);
  public readonly error = input<string | null>(null);
  public readonly send = output<string>();
}
