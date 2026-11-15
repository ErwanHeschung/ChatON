import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { PawTrail } from '@components/paw-trail/paw-trail';
import { CommunityStore } from '@stores/community.store';

/** Community menu → Invite: a fresh invite to copy and send. */
@Component({
  selector: 'app-invite-dialog',
  imports: [PawTrail],
  templateUrl: './invite-dialog.html',
  styleUrl: '../community-dialog/community-dialog.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InviteDialog implements OnInit {
  protected readonly store = inject(CommunityStore);
  public readonly invite = signal<string | null>(null);
  public readonly copied = signal(false);

  async ngOnInit(): Promise<void> {
    this.invite.set(await this.store.invite());
  }

  public copy(text: string): void {
    void navigator.clipboard.writeText(text).then(() => this.copied.set(true));
  }
}
