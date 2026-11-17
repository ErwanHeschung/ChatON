import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';
import { ShellService } from '@services/shell.service';

/**
 * Frame (community rail, sidebar) around a sheet (header, conversation, members).
 * Under 1024px members slide over the conversation; under 768px one screen at a time,
 * with the voice bar kept under the conversation while I'm in voice.
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
    '[class.voice]': 'voice()',
  },
})
export class ChatLayout {
  protected readonly shell = inject(ShellService);
  /** A voice bar is projected (`[voice]`): phones keep it under an open conversation. */
  public readonly voice = input(false);
}
