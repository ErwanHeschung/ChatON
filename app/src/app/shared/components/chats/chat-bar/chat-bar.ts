import { ChangeDetectionStrategy, Component, output } from '@angular/core';
import { LucideDynamicIcon, LucidePaperclip, LucideSmile } from '@lucide/angular';
import { EmojiSelect } from '@components/chats/emoji-select/emoji-select';
import { OverlayModule } from '@angular/cdk/overlay';

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

  public submit(input: HTMLInputElement): void {
    if (input.value.trim()) {
      this.send.emit(input.value);
      input.value = '';
    }
  }
}
