import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { CdkMenu, CdkMenuItem, CdkMenuTrigger } from '@angular/cdk/menu';
import {
  LucideChevronDown,
  LucideDynamicIcon,
  LucideSettings,
  LucideLogOut,
  LucideUserPlus,
} from '@lucide/angular';
import { InviteDialog } from '@components/servers/invite-dialog/invite-dialog';
import { LeaveDialog } from '@components/servers/leave-dialog/leave-dialog';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';

/** Sidebar header: the open community's name, and its menu (invite, settings, leave). */
@Component({
  selector: 'app-server-banner',
  imports: [CdkMenu, CdkMenuItem, CdkMenuTrigger, LucideDynamicIcon],
  templateUrl: './server-banner.html',
  styleUrl: './server-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerBanner {
  protected readonly store = inject(CommunityStore);
  private readonly modal = inject(ModalService);
  protected readonly shell = inject(ShellService);
  protected readonly icons = {
    chevron: LucideChevronDown,
    invite: LucideUserPlus,
    settings: LucideSettings,
    leave: LucideLogOut,
  };

  /** Same test as the settings' sections: one of them is mine to open. */
  protected readonly canSettings = computed(
    () =>
      this.store.isOwner() ||
      (['manage_community', 'manage_roles', 'kick_members'] as const).some((p) =>
        this.store.can(p),
      ),
  );

  public invite(): void {
    this.modal.open(InviteDialog);
  }

  public leave(): void {
    this.modal.open(LeaveDialog);
  }
}
