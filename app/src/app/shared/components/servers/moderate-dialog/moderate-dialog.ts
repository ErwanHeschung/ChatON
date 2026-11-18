import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { CmdError } from '@models/Auth.model';
import { DurationPipe } from '@pipes/duration.pipe';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { DmStore } from '@stores/dm.store';

/** Timeout lengths, in seconds (spec § 9: 28 days at most). */
const TIMEOUTS = [60, 300, 600, 3600, 86400, 604800];

/** Removes, bans or times out `shell.moderating`, with a reason for the log. */
@Component({
  selector: 'app-moderate-dialog',
  imports: [DurationPipe],
  templateUrl: './moderate-dialog.html',
  styleUrls: ['../community-dialog/community-dialog.scss', './moderate-dialog.scss'],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ModerateDialog {
  private readonly store = inject(CommunityStore);
  private readonly shell = inject(ShellService);
  protected readonly modal = inject(ModalService);
  protected readonly dm = inject(DmStore);

  protected readonly target = this.shell.moderating;
  protected readonly timeouts = TIMEOUTS;
  protected readonly state = signal<'idle' | 'saving' | CmdError['code']>('idle');
  protected readonly name = computed(() => {
    const t = this.target();
    return t ? this.dm.label(t.pubkey) : '';
  });

  protected async confirm(reason: string, length: string) {
    const t = this.target();
    if (!t) return;
    this.state.set('saving');
    const until = Math.floor(Date.now() / 1000) + Number(length);
    if (await this.store.moderate(t.action, t.pubkey, reason.trim(), until)) {
      this.modal.close();
    } else {
      this.state.set(this.store.error() ?? 'internal');
    }
  }
}
