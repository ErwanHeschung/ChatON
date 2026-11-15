import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { Message, MessageStatus } from '@models/Message.model';
import { ChatBubble } from '@components/chats/chat-bubble/chat-bubble';
import { ChatBar } from '@components/chats/chat-bar/chat-bar';

const CAT_LOREM = [
  'Meow.',
  'I have 14 cats and they all have tiny hats for Tuesday.',
  "Did you ever notice that if you stare at a cat staring at a wall, the wall starts staring back? It's the cat-pocalypse, I'm telling you! THE VIBES ARE WEIRD.",
  "Stray thought: what if cats are actually tiny aliens recording our secrets for their home planet 'Meow-tron 5'?",
  'PURRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRT.',
  'I just saw a cat riding a roomba while wearing a cape. My life is complete, yet I am terrified of what happens when they learn to open the fridge.',
  'FEED ME HUMAN. THE BOWL IS 10% EMPTY WHICH IS PHYSICALLY THE SAME AS BEING TOTALLY EMPTY.',
  "Absolute absolute absolute craziness is trying to bathe a calico on a full moon. Don't ask how I know. I still have the scars. The bubbles... so many bubbles.",
];

@Component({
  selector: 'app-chat',
  imports: [ChatBubble, ChatBar],
  templateUrl: './chat.html',
  styleUrl: './chat.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Chat {
  public readonly messages = input<Message[]>(
    Array.from({ length: 100 }).map((_, i) => {
      const randomText = CAT_LOREM[i % CAT_LOREM.length];

      return {
        id: i.toString(),
        senderId: i % 2 === 0 ? 'me' : 'them',
        text: randomText,
        timestamp: new Date(),
        status: MessageStatus.SENT,
      };
    }),
  );
}
