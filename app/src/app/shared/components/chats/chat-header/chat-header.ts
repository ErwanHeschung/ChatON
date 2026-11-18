import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { DatePipe } from '@angular/common';
import { ConnectedPosition, OverlayModule } from '@angular/cdk/overlay';
import {
  LucideArrowLeft,
  LucideDynamicIcon,
  LucideGlobe,
  LucideHash,
  LucideLock,
  LucidePhone,
  LucidePin,
  LucidePinOff,
  LucideUsers,
} from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/** Top of the sheet: what's open (channel, person, group), back on phones, call, members toggle. */
@Component({
  selector: 'app-chat-header',
  imports: [Avatar, DatePipe, LucideDynamicIcon, OverlayModule],
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
    network: LucideGlobe,
    pin: LucidePin,
    unpin: LucidePinOff,
  };
  protected readonly community = inject(CommunityStore);
  /** The open channel's pinned messages, newest first (spec § 9, Messages). */
  protected readonly pinned = computed(() =>
    this.community
      .messages()
      .filter((m) => m.pinned)
      .reverse(),
  );
  protected readonly canPin = computed(() => {
    const channel = this.community
      .current()
      ?.channels.find((c) => c.id === this.community.channel());
    return !!channel && this.community.canIn(channel, 'manage_messages');
  });
  protected readonly showPins = signal(false);
  /** Under the button, right-aligned. */
  protected readonly pinsAt: ConnectedPosition[] = [
    { originX: 'end', originY: 'bottom', overlayX: 'end', overlayY: 'top', offsetY: 8 },
  ];

  protected iso(at: number): string {
    return new Date(at * 1000).toISOString();
  }
  protected readonly callTitle = $localize`:@@header.call:Start a voice call`;
  protected readonly inCallTitle = $localize`:@@header.in-call:You're in this call`;
}
