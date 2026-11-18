import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { PawTrail } from '@components/paw-trail/paw-trail';
import { DurationPipe } from '@pipes/duration.pipe';
import { CommunityStore } from '@stores/community.store';

/** Expiry choices, in seconds; 0: never. */
const EXPIRIES = [1800, 3600, 21600, 86400, 604800, 0];
/** Max uses choices; 0: no limit. */
const USES = [0, 1, 5, 10, 25, 100];

/** Community menu → Invite: a fresh invite (7 days, no limit) to copy, or another with other limits. */
@Component({
  selector: 'app-invite-dialog',
  imports: [PawTrail, DurationPipe],
  templateUrl: './invite-dialog.html',
  styleUrl: '../community-dialog/community-dialog.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InviteDialog implements OnInit {
  protected readonly store = inject(CommunityStore);
  protected readonly expiries = EXPIRIES;
  protected readonly uses = USES;
  public readonly invite = signal<string | null>(null);
  public readonly copied = signal(false);

  ngOnInit(): void {
    void this.create(604800, 0);
  }

  protected async create(expiresIn: number, maxUses: number): Promise<void> {
    this.invite.set(null);
    this.copied.set(false);
    const expires = expiresIn ? Math.floor(Date.now() / 1000) + expiresIn : 0;
    this.invite.set(await this.store.invite(expires, maxUses));
  }

  public copy(text: string): void {
    void navigator.clipboard.writeText(text).then(() => this.copied.set(true));
  }
}
