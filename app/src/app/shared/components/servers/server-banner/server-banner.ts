import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import {
  CdkMenu,
  CdkMenuGroup,
  CdkMenuItem,
  CdkMenuItemRadio,
  CdkMenuTrigger,
} from '@angular/cdk/menu';
import {
  LucideBell,
  LucideCheck,
  LucideChevronDown,
  LucideFolderPlus,
  LucideDynamicIcon,
  LucideSettings,
  LucideLogOut,
  LucideUserPlus,
} from '@lucide/angular';
import { ChannelDialog } from '@components/servers/channel-dialog/channel-dialog';
import { InviteDialog } from '@components/servers/invite-dialog/invite-dialog';
import { LeaveDialog } from '@components/servers/leave-dialog/leave-dialog';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { NotifyLevel } from '@models/Community.model';

/** Sidebar header: the open community's name, and its menu (invite, settings, leave). */
@Component({
  selector: 'app-server-banner',
  imports: [
    CdkMenu,
    CdkMenuGroup,
    CdkMenuItem,
    CdkMenuItemRadio,
    CdkMenuTrigger,
    LucideDynamicIcon,
  ],
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
    category: LucideFolderPlus,
    leave: LucideLogOut,
    bell: LucideBell,
    check: LucideCheck,
  };
  protected readonly levels: { id: NotifyLevel; label: string }[] = [
    { id: 'all', label: $localize`:@@notify.all:All messages` },
    { id: 'mentions', label: $localize`:@@notify.mentions:Only mentions` },
    { id: 'none', label: $localize`:@@notify.none:Nothing` },
  ];

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

  public createCategory(): void {
    this.shell.editing.set({ category: '' });
    this.modal.open(ChannelDialog);
  }

  public leave(): void {
    this.modal.open(LeaveDialog);
  }
}
