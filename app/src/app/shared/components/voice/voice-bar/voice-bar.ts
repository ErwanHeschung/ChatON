import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import {
  LucideDynamicIcon,
  LucideHeadphoneOff,
  LucideHeadphones,
  LucideLock,
  LucideMic,
  LucideMicOff,
  LucidePhoneOff,
  LucideX,
} from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { ProfileDirective } from '@directives/profile';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/** Above my account while I'm in a voice channel or a call: where, and mute, deafen, hang up. */
@Component({
  selector: 'app-voice-bar',
  imports: [LucideDynamicIcon, Avatar, ProfileDirective],
  templateUrl: './voice-bar.html',
  styleUrl: './voice-bar.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '[class.shown]': 'voice.state() || voice.error()' },
})
export class VoiceBar {
  protected readonly voice = inject(VoiceStore);
  private readonly community = inject(CommunityStore);
  private readonly dm = inject(DmStore);
  private readonly user = inject(ConnectedUserStore);
  protected readonly icons = {
    mic: LucideMic,
    micOff: LucideMicOff,
    headphones: LucideHeadphones,
    headphonesOff: LucideHeadphoneOff,
    hangUp: LucidePhoneOff,
    lock: LucideLock,
    close: LucideX,
  };
  protected readonly e2e = $localize`:@@voice.e2e:End-to-end encrypted: your instance can't listen.`;
  protected readonly hangUpTitle = $localize`:@@voice.hang-up:Hang up`;
  protected readonly leaveTitle = $localize`:@@voice.leave:Leave voice`;

  /** In a call: the others, with who's speaking (a channel lists them under its row). */
  protected readonly callPeople = computed(() => {
    const state = this.voice.state();
    if (state?.target.type !== 'call') {
      return [];
    }
    const me = this.user.user()?.pubkey;
    return state.participants
      .filter((pk) => pk !== me)
      .map((pk) => ({
        id: pk,
        username: this.dm.label(pk),
        picture: this.dm.peers()[pk]?.picture,
        speaking: state.speaking.includes(pk),
      }));
  });

  /** My call, nobody else in it yet. */
  protected readonly ringingOut = computed(
    () => this.voice.state()?.target.type === 'call' && !this.callPeople().length,
  );

  /** "Salon · Community", or the conversation's name. */
  protected readonly place = computed(() => {
    const target = this.voice.state()?.target;
    if (target?.type === 'call') {
      return this.dm.title(target.room.join(','));
    }
    const community = this.community.communities().find((c) => c.id === target?.community);
    const channel = community?.channels.find((c) => c.id === target?.channel);
    return channel && community ? `${channel.name} · ${community.name}` : '';
  });
}
