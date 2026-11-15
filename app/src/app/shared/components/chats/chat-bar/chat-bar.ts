import { ChangeDetectionStrategy, Component, output } from '@angular/core';
import { LucideDynamicIcon, LucideSmile } from '@lucide/angular';
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
  public readonly send = output<string>();

  public submit(input: HTMLInputElement): void {
    if (input.value.trim()) {
      this.send.emit(input.value);
      input.value = '';
    }
  }
}
