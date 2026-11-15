import {
  ChangeDetectionStrategy,
  Component,
  inject,
  OnDestroy,
  OnInit,
  signal,
} from '@angular/core';
import { LucideDynamicIcon, LucideHash, LucidePlus } from '@lucide/angular';
import { PawTrail } from '@components/paw-trail/paw-trail';
import { ServerBanner } from '@components/servers/server-banner/server-banner';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';

/** Sidebar of the server view: the open community's channels. */
@Component({
  selector: 'app-server-channels',
  imports: [LucideDynamicIcon, PawTrail, ServerBanner],
  templateUrl: './server-channels.html',
  styleUrl: './server-channels.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerChannels implements OnInit, OnDestroy {
  protected readonly store = inject(CommunityStore);
  private readonly shell = inject(ShellService);

  public readonly hashtagIcon = LucideHash;
  public readonly plusIcon = LucidePlus;
  /** Admin: the "new channel" field is shown. */
  public readonly adding = signal(false);

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

  public open(channel: string): void {
    void this.store.selectChannel(channel);
    this.shell.openChat();
  }

  public addChannel(input: HTMLInputElement): void {
    const name = input.value.trim();
    if (name) {
      void this.store.createChannel(name).then(() => {
        input.value = '';
        this.adding.set(false);
      });
    }
  }
}
