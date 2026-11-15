import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { ModalService } from '@services/modal.service';
import { CommunityStore } from '@stores/community.store';

/** Community menu → Leave: confirms first. */
@Component({
  selector: 'app-leave-dialog',
  templateUrl: './leave-dialog.html',
  styleUrl: '../community-dialog/community-dialog.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LeaveDialog {
  protected readonly store = inject(CommunityStore);
  protected readonly modal = inject(ModalService);

  public leave(): void {
    this.modal.close();
    void this.store.leave();
  }
}
