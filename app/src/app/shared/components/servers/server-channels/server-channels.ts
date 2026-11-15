import {
  ChangeDetectionStrategy,
  Component,
  inject,
  OnDestroy,
  OnInit,
  signal,
} from '@angular/core';
import { LucideDynamicIcon, LucideHash, LucidePlus } from '@lucide/angular';
import { ServerBanner } from '@components/servers/server-banner/server-banner';
import { CommunityStore } from '@stores/community.store';

/** Left sidebar of the server view: channels of the open community, invites. */
@Component({
  selector: 'app-server-channels',
  imports: [LucideDynamicIcon, ServerBanner],
  templateUrl: './server-channels.html',
  styleUrl: './server-channels.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerChannels implements OnInit, OnDestroy {
  protected readonly store = inject(CommunityStore);

  public readonly hashtagIcon = LucideHash;
  public readonly plusIcon = LucidePlus;
  public readonly invite = signal<string | null>(null);
  public readonly copied = signal(false);

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

  public addChannel(input: HTMLInputElement): void {
    const name = input.value.trim();
    if (name) {
      void this.store.createChannel(name).then(() => (input.value = ''));
    }
  }

  public async createInvite(): Promise<void> {
    this.copied.set(false);
    this.invite.set(await this.store.invite());
  }

  public copy(text: string): void {
    void navigator.clipboard.writeText(text).then(() => this.copied.set(true));
  }
}
