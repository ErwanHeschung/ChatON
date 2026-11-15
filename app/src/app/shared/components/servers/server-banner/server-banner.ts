import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { CdkMenu, CdkMenuItem, CdkMenuTrigger } from '@angular/cdk/menu';
import {
  LucideChevronDown,
  LucideDynamicIcon,
  LucideImage,
  LucideLogOut,
  LucideUserPlus,
} from '@lucide/angular';
import { InviteDialog } from '@components/servers/invite-dialog/invite-dialog';
import { LeaveDialog } from '@components/servers/leave-dialog/leave-dialog';
import { ModalService } from '@services/modal.service';
import { CommunityStore } from '@stores/community.store';

/** Sidebar header: the open community's name, and its menu (invite, picture, leave). */
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
  protected readonly icons = {
    chevron: LucideChevronDown,
    invite: LucideUserPlus,
    picture: LucideImage,
    leave: LucideLogOut,
  };

  public invite(): void {
    this.modal.open(InviteDialog);
  }

  public setPicture(input: HTMLInputElement): void {
    const file = input.files?.[0];
    input.value = '';
    if (file) void this.store.setPicture(file);
  }

  public leave(): void {
    this.modal.open(LeaveDialog);
  }
}
