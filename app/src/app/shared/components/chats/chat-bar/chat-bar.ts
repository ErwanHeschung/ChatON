import { ChangeDetectionStrategy, Component } from '@angular/core';
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
}
