import {
  afterNextRender,
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  effect,
  ElementRef,
  inject,
} from '@angular/core';
import { LucideDynamicIcon, LucidePhone, LucidePhoneOff } from '@lucide/angular';
import { UserCard } from '@components/users/user-card/user-card';
import { UserShort } from '@models/User.model';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/**
 * Someone calls me: who, Answer, Decline. Floats at the top until it stops ringing.
 * Takes the focus (not on a button: a keystroke meant for the composer must not answer),
 * Escape declines, and the focus goes back where it was.
 */
@Component({
  selector: 'app-call-banner',
  imports: [LucideDynamicIcon, UserCard],
  templateUrl: './call-banner.html',
  styleUrl: './call-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    role: 'alertdialog',
    'aria-labelledby': 'call-who',
    tabindex: '-1',
    '(keydown.escape)': 'voice.decline()',
  },
})
export class CallBanner {
  protected readonly voice = inject(VoiceStore);
  private readonly dm = inject(DmStore);
  private readonly user = inject(ConnectedUserStore);
  protected readonly icons = { answer: LucidePhone, decline: LucidePhoneOff };

  protected readonly caller = computed<UserShort | null>(() => {
    const sender = this.voice.ring()?.message.sender;
    return sender
      ? { id: sender, username: this.dm.label(sender), picture: this.dm.peers()[sender]?.picture }
      : null;
  });

  private readonly room = computed(() => this.voice.ring()?.message.room ?? []);

  /** A named group: its name. */
  protected readonly group = computed(() => {
    const key = this.room().join(',');
    return this.room().length > 2
      ? (this.dm.conversations().find((c) => c.room.join(',') === key)?.subject ?? null)
      : null;
  });

  /** An unnamed group: the others called, not the caller nor me. */
  protected readonly others = computed(() => {
    const room = this.room();
    const skip = [this.voice.ring()?.message.sender, this.user.user()?.pubkey];
    return room.length > 2
      ? room
          .filter((pk) => !skip.includes(pk))
          .map((pk) => this.dm.label(pk))
          .join(', ')
      : null;
  });

  constructor() {
    const host = inject(ElementRef<HTMLElement>).nativeElement as HTMLElement;
    const before = document.activeElement as HTMLElement | null;
    afterNextRender(() => host.focus());
    inject(DestroyRef).onDestroy(() => before?.focus());

    effect(() => {
      const ring = this.voice.ring();
      ring?.message.room.forEach((pk) => void this.dm.resolve(pk));
    });
  }

  public answer(): void {
    const message = this.voice.ring()?.message;
    if (message?.call) void this.voice.joinCall(message.room, message.call.id);
  }
}
