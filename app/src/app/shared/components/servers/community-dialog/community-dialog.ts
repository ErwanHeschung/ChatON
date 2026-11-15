import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { ROUTES } from '@app/app.routes';
import { ModalService } from '@services/modal.service';
import { CommunityStore } from '@stores/community.store';

/** "+" of the community rail: create a community, or join one with an invite. */
@Component({
  selector: 'app-community-dialog',
  templateUrl: './community-dialog.html',
  styleUrl: './community-dialog.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CommunityDialog {
  protected readonly store = inject(CommunityStore);
  private readonly modal = inject(ModalService);
  private readonly router = inject(Router);

  public readonly busy = signal(false);

  public async submit(action: 'create' | 'join', input: HTMLInputElement): Promise<void> {
    const value = input.value.trim();
    if (!value || this.busy()) {
      return;
    }
    this.busy.set(true);
    const ok = await (action === 'create' ? this.store.create(value) : this.store.join(value));
    this.busy.set(false);
    if (ok) {
      this.modal.close();
      void this.router.navigate([ROUTES.CHAT, ROUTES.SERVERS]);
    }
  }
}
