import {
  ChangeDetectionStrategy,
  Component,
  effect,
  inject,
  OnDestroy,
  OnInit,
  signal,
} from '@angular/core';
import { LucideDynamicIcon, LucideHash, LucidePlus, LucideVolume2 } from '@lucide/angular';
import { PawTrail } from '@components/paw-trail/paw-trail';
import { ServerBanner } from '@components/servers/server-banner/server-banner';
import { UserCard } from '@components/users/user-card/user-card';
import { ProfileDirective } from '@directives/profile';
import { Channel } from '@models/Community.model';
import { UserShort } from '@models/User.model';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/** Sidebar of the server view: the open community's channels, text and voice. */
@Component({
  selector: 'app-server-channels',
  imports: [LucideDynamicIcon, PawTrail, ServerBanner, UserCard, ProfileDirective],
  templateUrl: './server-channels.html',
  styleUrl: './server-channels.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerChannels implements OnInit, OnDestroy {
  protected readonly store = inject(CommunityStore);
  protected readonly voice = inject(VoiceStore);
  private readonly dm = inject(DmStore);
  private readonly user = inject(ConnectedUserStore);
  private readonly shell = inject(ShellService);

  public readonly icons = { hash: LucideHash, plus: LucidePlus, voice: LucideVolume2 };
  protected readonly joinTitle = $localize`:@@channels.join-voice:Join the voice channel`;
  protected readonly insideTitle = $localize`:@@channels.in-voice:You're in this voice channel`;
  /** Admin: the "new channel" field is shown. */
  public readonly adding = signal(false);
  /** The new channel is a voice channel. */
  public readonly addingVoice = signal(false);

  constructor() {
    effect(() => {
      const community = this.store.selected();
      if (community) void this.voice.checkAvailable(community);
    });
    // Handles of whoever is in a voice channel.
    effect(() => {
      for (const channel of this.store.current()?.channels ?? []) {
        channel.participants.forEach((pk) => void this.dm.resolve(pk));
      }
    });
  }

  ngOnInit(): void {
    this.store
      .load()
      .then(() => this.store.select())
      .catch(() => undefined);
  }

  ngOnDestroy(): void {
    // Back to DMs: the chat shows the DM again.
    void this.store.selectChannel(null);
  }

  public open(channel: Channel): void {
    const community = this.store.selected();
    if (channel.voice) {
      if (community && this.voice.channel() !== channel.id) {
        void this.voice.joinChannel(community, channel.id);
      }
      return;
    }
    void this.store.selectChannel(channel.id);
    this.shell.openChat();
  }

  /** Live from my own room when I'm in it, else from the relay (39004). */
  public participants(channel: Channel): UserShort[] {
    const state = this.voice.state();
    const live = this.voice.channel() === channel.id && state ? state.participants : null;
    return (live ?? channel.participants).map((pk) => ({
      id: pk,
      username: this.dm.label(pk),
      picture: this.dm.peers()[pk]?.picture,
    }));
  }

  public speaking(pubkey: string): boolean {
    return !!this.voice.state()?.speaking.includes(pubkey);
  }

  public isMe(pubkey: string): boolean {
    return pubkey === this.user.user()?.pubkey;
  }

  public addChannel(input: HTMLInputElement): void {
    const name = input.value.trim();
    if (name) {
      void this.store.createChannel(name, this.addingVoice()).then(() => {
        input.value = '';
        this.adding.set(false);
        this.addingVoice.set(false);
      });
    }
  }
}
