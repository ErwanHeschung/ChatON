import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';
import { LucideAtSign, LucideDynamicIcon, LucidePaperclip, LucideSmile } from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { MentionCandidate } from '@models/Message.model';
import { EmojiSelect } from '@components/chats/emoji-select/emoji-select';
import { ConnectedPosition, OverlayModule } from '@angular/cdk/overlay';

@Component({
  selector: 'app-chat-bar',
  imports: [Avatar, LucideDynamicIcon, EmojiSelect, OverlayModule],
  templateUrl: './chat-bar.html',
  styleUrl: './chat-bar.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChatBar {
  public readonly emojiIcon = LucideSmile;
  public isPickerOpen = false;
  public readonly attachIcon = LucidePaperclip;
  /** What's open, for the placeholder: "Message #général", "Message @lea@chaton.fr". */
  public readonly target = input('');
  public readonly channel = input(false);
  protected readonly placeholder = computed(() =>
    this.channel()
      ? $localize`:@@chat.message-to-channel:Message #${this.target()}:name:`
      : $localize`:@@chat.message-to:Message ${this.target()}:name:`,
  );
  /** Above the button, right-aligned with it. */
  protected readonly pickerPositions: ConnectedPosition[] = [
    { originX: 'end', originY: 'top', overlayX: 'end', overlayY: 'bottom', offsetY: -8 },
  ];
  public readonly send = output<{ text: string; mentions: string[] }>();
  /** Who `@` may mention here (channels; none in DMs). */
  public readonly candidates = input<MentionCandidate[]>([]);
  protected readonly atIcon = LucideAtSign;
  /** What follows the `@` before the caret; null: not mentioning. */
  protected readonly query = signal<string | null>(null);
  protected readonly active = signal(0);
  protected readonly matches = computed(() => {
    const q = this.query()?.toLowerCase();
    if (q === undefined) return [];
    return this.candidates()
      .filter((c) => c.text.toLowerCase().includes(q) || c.name.toLowerCase().includes(q))
      .slice(0, 8);
  });
  /** Mentions picked, by their text: kept if the text is still there when sent. */
  private readonly picked = new Map<string, string>();

  protected typed(input: HTMLInputElement): void {
    const before = input.value.slice(0, input.selectionStart ?? input.value.length);
    this.query.set(/(?:^|\s)@(\S*)$/.exec(before)?.[1] ?? null);
    this.active.set(0);
  }

  protected key(event: KeyboardEvent, input: HTMLInputElement): void {
    const n = this.matches().length;
    if (n && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
      event.preventDefault();
      this.active.update((i) => (i + (event.key === 'ArrowDown' ? 1 : n - 1)) % n);
    } else if (n && (event.key === 'Enter' || event.key === 'Tab')) {
      event.preventDefault();
      this.choose(input, this.matches()[this.active()]);
    } else if (n && event.key === 'Escape') {
      this.query.set(null);
    } else if (event.key === 'Enter') {
      this.submit(input);
    }
  }

  /** Replaces the `@…` being typed by the candidate's text. */
  protected choose(input: HTMLInputElement, c: MentionCandidate): void {
    const caret = input.selectionStart ?? input.value.length;
    const start = input.value.lastIndexOf('@', caret - 1);
    input.setRangeText(`${c.text} `, start, caret, 'end');
    this.picked.set(c.text, c.id);
    this.query.set(null);
  }
  public readonly file = output<File>();

  public pick(input: HTMLInputElement): void {
    const file = input.files?.[0];
    input.value = '';
    if (file) {
      this.file.emit(file);
    }
  }

  /** A pasted image (or file) is sent like a picked one. */
  public paste(event: ClipboardEvent): void {
    const file = event.clipboardData?.files[0];
    if (file) {
      event.preventDefault();
      this.file.emit(file);
    }
  }

  /** At the caret (replacing a selection); the picker stays open for more. */
  public insert(input: HTMLInputElement, emoji: string): void {
    const end = input.value.length;
    input.setRangeText(emoji, input.selectionStart ?? end, input.selectionEnd ?? end, 'end');
  }

  public submit(input: HTMLInputElement): void {
    const text = input.value;
    if (text.trim()) {
      const mentions = [...this.picked].filter(([t]) => text.includes(t)).map(([, id]) => id);
      this.send.emit({ text, mentions });
      input.value = '';
      this.picked.clear();
    }
  }
}
