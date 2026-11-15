import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { CommunityStore } from '@stores/community.store';

/** Name of the open community, and leaving it. */
@Component({
  selector: 'app-server-banner',
  templateUrl: './server-banner.html',
  styleUrl: './server-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerBanner {
  protected readonly store = inject(CommunityStore);
  public readonly confirmLeave = signal(false);

  public leave(): void {
    this.confirmLeave.set(false);
    void this.store.leave();
  }
}
