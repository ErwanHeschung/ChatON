import { computed, inject, Injectable, signal } from '@angular/core';
import { splitHandle } from '@models/Dm.model';
import { CommunityStore } from '@stores/community.store';
import { DmStore } from '@stores/dm.store';

/** What the open conversation is, for its header, intro and composer. */
export type OpenConversation =
  | { kind: 'channel'; title: string }
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
  /** Members panel: shown by default wherever it sits beside the conversation (not over it). */
  public readonly members = signal(globalThis.matchMedia?.('(min-width: 1024px)').matches ?? true);

  public readonly conversation = computed<OpenConversation | null>(() => {
    const id = this.community.channel();
    const channel = this.community.current()?.channels.find((c) => c.id === id);
    if (channel) {
      return { kind: 'channel', title: channel.name };
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
