import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import {
  LucideArrowLeft,
  LucideDynamicIcon,
  LucideHash,
  LucideLock,
  LucideUsers,
} from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { ShellService } from '@services/shell.service';

/** Top of the sheet: what's open (channel, person, group), back on phones, members toggle. */
@Component({
  selector: 'app-chat-header',
  imports: [Avatar, LucideDynamicIcon],
  templateUrl: './chat-header.html',
  styleUrl: './chat-header.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChatHeader {
  protected readonly shell = inject(ShellService);
  protected readonly icons = {
    back: LucideArrowLeft,
    hash: LucideHash,
    lock: LucideLock,
    members: LucideUsers,
  };
}
