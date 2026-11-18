import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  OnDestroy,
  OnInit,
  signal,
} from '@angular/core';
import {
  CdkDrag,
  CdkDragDrop,
  CdkDropList,
  CdkDropListGroup,
  moveItemInArray,
  transferArrayItem,
} from '@angular/cdk/drag-drop';
import {
  LucideChevronDown,
  LucideDynamicIcon,
  LucideHash,
  LucidePlus,
  LucideSettings,
  LucideVolume2,
} from '@lucide/angular';
import { ChannelDialog } from '@components/servers/channel-dialog/channel-dialog';
import { PawTrail } from '@components/paw-trail/paw-trail';
import { ServerBanner } from '@components/servers/server-banner/server-banner';
import { UserCard } from '@components/users/user-card/user-card';
import { ProfileDirective } from '@directives/profile';
import { Category, Channel } from '@models/Community.model';
import { UserShort } from '@models/User.model';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/**
 * Sidebar of the server view: the open community's channels, text and voice, in their
 * categories. With manage_channels: add, set up, and drag to arrange (spec § 9).
 */
@Component({
  selector: 'app-server-channels',
  imports: [
    CdkDrag,
    CdkDropList,
    CdkDropListGroup,
    LucideDynamicIcon,
    PawTrail,
    ServerBanner,
    UserCard,
    ProfileDirective,
  ],
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
  private readonly modal = inject(ModalService);

  public readonly icons = {
    hash: LucideHash,
    plus: LucidePlus,
    voice: LucideVolume2,
    settings: LucideSettings,
    chevron: LucideChevronDown,
  };
  protected readonly joinTitle = $localize`:@@channels.join-voice:Join the voice channel`;
  protected readonly insideTitle = $localize`:@@channels.in-voice:You're in this voice channel`;
  /** The section ('' above the categories, or a category id) showing the "new channel" field. */
  public readonly adding = signal<string | null>(null);
  /** The new channel is a voice channel. */
  public readonly addingVoice = signal(false);

  protected readonly manage = computed(() => this.store.can('manage_channels'));
  /** Categories folded on this device. */
  protected readonly collapsed = signal(new Set<string>());

  /** Channels above the categories (id ''), then each category with its own. */
  protected readonly sections = computed(() => {
    const c = this.store.current();
    if (!c) return [];
    const known = (ch: Channel) => c.categories.some((k) => k.id === ch.category);
    return [
      {
        id: '',
        category: null as Category | null,
        channels: c.channels.filter((ch) => !known(ch)),
      },
      ...c.categories.map((k) => ({
        id: k.id,
        category: k as Category | null,
        channels: c.channels.filter((ch) => ch.category === k.id),
      })),
    ];
  });

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

  /** Shares a screen (known once I'm in the channel). */
  public live(pubkey: string): boolean {
    return !!this.voice.state()?.sharing.includes(pubkey);
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
      void this.store.createChannel(name, this.addingVoice(), this.adding() || null).then((ok) => {
        if (!ok) return;
        input.value = '';
        this.adding.set(null);
        this.addingVoice.set(false);
      });
    }
  }

  protected toggleAdding(section: string): void {
    this.adding.update((open) => (open === section ? null : section));
    this.collapsed.update((s) => (s.delete(section) ? new Set(s) : s));
  }

  protected toggleCategory(id: string): void {
    this.collapsed.update((s) => {
      const next = new Set(s);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  /** Still shown in a folded category: the open channel, the voice room I'm in. */
  protected active(channel: Channel): boolean {
    return channel.id === this.store.channel() || channel.id === this.voice.channel();
  }

  protected editChannel(channel: string): void {
    this.shell.editing.set({ channel });
    this.modal.open(ChannelDialog);
  }

  protected editCategory(category: string): void {
    this.shell.editing.set({ category });
    this.modal.open(ChannelDialog);
  }

  /** A channel dropped in a section: the whole order goes to the relay. */
  protected drop(e: CdkDragDrop<string>): void {
    const sections = this.sections().map((s) => ({ id: s.id, ids: s.channels.map((c) => c.id) }));
    const from = sections.find((s) => s.id === e.previousContainer.data);
    const to = sections.find((s) => s.id === e.container.data);
    if (!from || !to || (from === to && e.previousIndex === e.currentIndex)) return;
    if (from === to) {
      moveItemInArray(to.ids, e.previousIndex, e.currentIndex);
    } else {
      transferArrayItem(from.ids, to.ids, e.previousIndex, e.currentIndex);
    }
    void this.store.arrange(
      sections.flatMap((s) => s.ids.map((id) => ({ id, category: s.id || null }))),
    );
  }
}
