import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { LucideDynamicIcon, LucideShieldAlert } from '@lucide/angular';
import { instanceOf, splitHandle } from '@models/Dm.model';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** Above the composer of a message request: who's asking, and a way to block them. */
@Component({
  selector: 'app-request-banner',
  imports: [LucideDynamicIcon],
  templateUrl: './request-banner.html',
  styleUrl: './request-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RequestBanner {
  protected readonly dm = inject(DmStore);
  private readonly user = inject(ConnectedUserStore);
  protected readonly icon = LucideShieldAlert;

  protected readonly sender = computed(() => this.dm.request()?.last.sender ?? '');
  protected readonly handle = computed(() => splitHandle(this.dm.label(this.sender())));
  /** Their instance, when it's not mine: it can be blocked as a whole. */
  protected readonly instance = computed(() => {
    const theirs = instanceOf(this.dm.peers()[this.sender()]?.handle);
    return theirs !== instanceOf(this.user.user()?.handle) ? theirs : null;
  });

  protected block(target: { key?: string; instance?: string }): void {
    void this.dm.setBlocked(target, true);
  }
}
