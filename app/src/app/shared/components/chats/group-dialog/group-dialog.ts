import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { invoke } from '@tauri-apps/api/core';
import { ROUTES } from '@app/app.routes';
import { CmdError } from '@models/Auth.model';
import { Peer } from '@models/Dm.model';
import { ModalService } from '@services/modal.service';
import { DmStore } from '@stores/dm.store';

/** New group: a name and `@pseudo@instance` handles. "Add people" starts from a group's members. */
@Component({
  selector: 'app-group-dialog',
  templateUrl: './group-dialog.html',
  styleUrl: '../../servers/community-dialog/community-dialog.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class GroupDialog {
  protected readonly dm = inject(DmStore);
  private readonly modal = inject(ModalService);
  private readonly router = inject(Router);

  public readonly busy = signal(false);
  public readonly error = signal<CmdError['code'] | null>(null);
  /** The handle that wasn't found. */
  public readonly unknown = signal('');
  public readonly draft = computed(() => this.dm.groupDraft().map(this.dm.label).join(', '));

  public async submit(name: HTMLInputElement, handles: HTMLTextAreaElement): Promise<void> {
    const subject = name.value.trim();
    if (!subject || this.busy()) {
      return;
    }
    this.busy.set(true);
    this.error.set(null);
    const members = [...this.dm.groupDraft()];
    for (const handle of handles.value.split(/[\s,]+/).filter(Boolean)) {
      try {
        members.push((await invoke<Peer>('search_handle', { handle })).pubkey);
      } catch (e) {
        this.unknown.set(handle);
        this.error.set((e as CmdError).code ?? 'internal');
        this.busy.set(false);
        return;
      }
    }
    const error = await this.dm.createGroup(members, subject);
    this.busy.set(false);
    this.error.set(error);
    if (!error) {
      this.dm.setGroupDraft([]);
      this.modal.close();
      void this.router.navigate([ROUTES.CHAT, ROUTES.DMS]);
    }
  }
}
