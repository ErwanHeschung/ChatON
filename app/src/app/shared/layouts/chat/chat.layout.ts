import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { ShellService } from '@services/shell.service';

/**
 * Frame (community rail, sidebar) around a sheet (header, conversation, members).
 * Under 1024px members slide over the conversation; under 768px one screen at a time.
 */
@Component({
  selector: 'app-chat-layout',
  imports: [],
  templateUrl: './chat.layout.html',
  styleUrl: './chat.layout.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '[class.chat-open]': 'shell.chatOpen()',
    '[class.members-open]': 'shell.members()',
  },
})
export class ChatLayout {
  protected readonly shell = inject(ShellService);
}
