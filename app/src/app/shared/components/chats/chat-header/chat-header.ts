import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import {
  LucideArrowLeft,
  LucideDynamicIcon,
  LucideHash,
  LucideLock,
  LucidePhone,
  LucideUsers,
} from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { ShellService } from '@services/shell.service';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/** Top of the sheet: what's open (channel, person, group), back on phones, call, members toggle. */
@Component({
  selector: 'app-chat-header',
  imports: [Avatar, LucideDynamicIcon],
  templateUrl: './chat-header.html',
  styleUrl: './chat-header.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChatHeader {
  protected readonly shell = inject(ShellService);
  protected readonly voice = inject(VoiceStore);
  protected readonly dm = inject(DmStore);
  protected readonly icons = {
    call: LucidePhone,
    back: LucideArrowLeft,
    hash: LucideHash,
    lock: LucideLock,
    members: LucideUsers,
  };
  protected readonly callTitle = $localize`:@@header.call:Start a voice call`;
  protected readonly inCallTitle = $localize`:@@header.in-call:You're in this call`;
}
