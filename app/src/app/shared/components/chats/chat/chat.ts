import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { DatePipe, NgOptimizedImage } from '@angular/common';
import { LucideDynamicIcon, LucideUsers, LucideX } from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { ChatBar } from '@components/chats/chat-bar/chat-bar';
import { ChatBubble } from '@components/chats/chat-bubble/chat-bubble';
import { PawTrail } from '@components/paw-trail/paw-trail';
import { MentionCandidate, Message } from '@models/Message.model';
import { ShellService } from '@services/shell.service';

/** A run of messages by one author stays grouped if each follows the previous within this. */
const GROUP_GAP_MS = 7 * 60 * 1000;

const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();

/** "today", "yesterday", or the date itself, for the separator above a day's first message. */
function dayLabel(date: Date): 'today' | 'yesterday' | Date {
  const yesterday = new Date();
  yesterday.setDate(yesterday.getDate() - 1);
  if (sameDay(date, new Date())) return 'today';
  if (sameDay(date, yesterday)) return 'yesterday';
  return date;
}

@Component({
  selector: 'app-chat',
  imports: [ChatBubble, ChatBar, Avatar, DatePipe, LucideDynamicIcon, NgOptimizedImage, PawTrail],
  templateUrl: './chat.html',
  styleUrl: './chat.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Chat {
  protected readonly shell = inject(ShellService);
  protected readonly icons = { group: LucideUsers, close: LucideX };
  /** Newest first: the list is rendered bottom-up (`column-reverse`). */
  public readonly messages = input<Message[]>([]);
  public readonly disabled = input(false);
  public readonly error = input<string | null>(null);
  public readonly send = output<{ text: string; mentions: string[] }>();
  /** Who `@` may mention (channels). */
  public readonly candidates = input<MentionCandidate[]>([]);
  public readonly file = output<File>();
  /** Id of a message to delete. */
  public readonly remove = output<string>();
  /** Join the call a note announces (its message). */
  public readonly join = output<Message>();
  /** Whom the next message answers (their name), if any. */
  public readonly replying = input<string | null>(null);
  /** Answer a message (its id); null: not anymore. */
  public readonly reply = output<string | null>();
  public readonly react = output<{ id: string; emoji: string; on: boolean }>();
  public readonly pin = output<{ id: string; pinned: boolean }>();

  /** Each message, whether it continues its author's run, and the day it opens (if any). */
  protected readonly rows = computed(() => {
    const messages = this.messages();
    return messages.map((message, i) => {
      const older = messages[i + 1];
      const newDay = !older || !sameDay(older.timestamp, message.timestamp);
      const continued =
        !!older &&
        !newDay &&
        older.senderId === message.senderId &&
        !older.subject &&
        !message.subject &&
        !older.call &&
        !message.call &&
        !message.reply &&
        !message.pinned &&
        message.timestamp.getTime() - older.timestamp.getTime() < GROUP_GAP_MS;
      return { message, continued, day: newDay ? dayLabel(message.timestamp) : null };
    });
  });
}
