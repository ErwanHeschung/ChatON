import { computed, effect, inject, Injectable, signal, untracked } from '@angular/core';
import { splitHandle } from '@models/Dm.model';
import { CommunityStore } from '@stores/community.store';
import { DmStore } from '@stores/dm.store';

/** What the open conversation is, for its header, intro and composer. */
export type OpenConversation =
  | { kind: 'channel'; title: string; topic: string | null }
  | { kind: 'group'; title: string; count: number }
  | {
      kind: 'person';
      title: string;
      handle: [string, string];
      pubkey: string;
      picture: string | null | undefined;
    };

/** The chat shell: what's open, and what it shows when it can't show everything. */
@Injectable({ providedIn: 'root' })
export class ShellService {
  private readonly community = inject(CommunityStore);
  private readonly dm = inject(DmStore);

  /** Phones: the conversation instead of the lists, after a tap on a row. */
  public readonly chatOpen = signal(false);
  /** Settings, full screen over the shell. */
  public readonly settings = signal(false);
  /** The open community's settings, full screen too. */
  public readonly serverSettings = signal(false);
  /** The channel or category whose dialog is open ('' : a new category). */
  public readonly editing = signal<{ channel: string } | { category: string } | null>(null);
  /** The network map in the sheet, until a conversation or channel is opened. */
  public readonly network = signal(false);
  /** Members panel: shown by default wherever it sits beside the conversation (not over it). */
  public readonly members = signal(globalThis.matchMedia?.('(min-width: 1024px)').matches ?? true);

  public readonly conversation = computed<OpenConversation | null>(() => {
    const id = this.community.channel();
    const channel = this.community.current()?.channels.find((c) => c.id === id);
    if (channel) {
      return { kind: 'channel', title: channel.name, topic: channel.topic };
    }
    const key = this.dm.selected();
    if (!key) {
      return null;
    }
    if (this.dm.isGroup()) {
      return { kind: 'group', title: this.dm.title(key), count: this.dm.room().length };
    }
    const peer = this.dm.others(key)[0] ?? key;
    const label = this.dm.label(peer);
    return {
      kind: 'person',
      title: label,
      handle: splitHandle(label),
      pubkey: peer,
      picture: this.dm.peers()[peer]?.picture,
    };
  });

  constructor() {
    effect(() => {
      this.dm.selected();
      this.community.channel();
      untracked(() => this.network.set(false));
    });
  }

  public openChat(): void {
    this.chatOpen.set(true);
  }

  public back(): void {
    this.chatOpen.set(false);
  }

  public toggleMembers(): void {
    this.members.update((shown) => !shown);
  }
}
