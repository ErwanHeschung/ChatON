import { ChangeDetectionStrategy, Component } from '@angular/core';
import { LucideAngularModule, Smile } from 'lucide-angular';

@Component({
  selector: 'app-chat-bar',
  imports: [LucideAngularModule],
  templateUrl: './chat-bar.html',
  styleUrl: './chat-bar.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChatBar {
  public readonly emojiIcon = Smile;
}
