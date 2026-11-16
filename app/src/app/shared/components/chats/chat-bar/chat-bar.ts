import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { LucideDynamicIcon, LucidePaperclip, LucideSmile } from '@lucide/angular';
import { EmojiSelect } from '@components/chats/emoji-select/emoji-select';
import { ConnectedPosition, OverlayModule } from '@angular/cdk/overlay';

@Component({
  selector: 'app-chat-bar',
  imports: [LucideDynamicIcon, EmojiSelect, OverlayModule],
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
  public readonly send = output<string>();
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
    if (input.value.trim()) {
      this.send.emit(input.value);
      input.value = '';
    }
  }
}
