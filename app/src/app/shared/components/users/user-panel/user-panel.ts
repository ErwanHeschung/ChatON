import { ChangeDetectionStrategy, Component, computed, effect, inject } from '@angular/core';
import { CdkMenu, CdkMenuItemRadio, CdkMenuTrigger } from '@angular/cdk/menu';
import { ConnectedPosition } from '@angular/cdk/overlay';
import { LucideCheck, LucideDynamicIcon, LucideSettings } from '@lucide/angular';
import { StatusDot } from '@components/users/status-dot/status-dot';
import { UserCard } from '@components/users/user-card/user-card';
import { STATUS_LABELS, Status } from '@models/Presence.model';
import { UserShort } from '@models/User.model';
import { ShellService } from '@services/shell.service';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { PresenceStore } from '@stores/presence.store';

/** Bottom of the sidebar: who I am, my status, and my account settings. */
@Component({
  selector: 'app-user-panel',
  imports: [UserCard, StatusDot, LucideDynamicIcon, CdkMenu, CdkMenuItemRadio, CdkMenuTrigger],
  templateUrl: './user-panel.html',
  styleUrl: './user-panel.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserPanel {
  private readonly account = inject(ConnectedUserStore);
  private readonly dm = inject(DmStore);
  private readonly shell = inject(ShellService);
  protected readonly presence = inject(PresenceStore);
  protected readonly icons = { settings: LucideSettings, check: LucideCheck };
  protected readonly labels = STATUS_LABELS;
  protected readonly statuses: { id: Status; hint?: string }[] = [
    { id: 'online' },
    { id: 'idle' },
    {
      id: 'dnd',
      hint: $localize`:@@presence.dnd-hint:No notifications, on any of your devices`,
    },
    { id: 'invisible', hint: $localize`:@@presence.invisible-hint:You appear offline` },
  ];
  /** The panel is at the bottom: the menu opens above it. */
  protected readonly above: ConnectedPosition[] = [
    { originX: 'start', originY: 'top', overlayX: 'start', overlayY: 'bottom', offsetY: -8 },
  ];

  protected readonly me = computed<UserShort>(() => {
    const user = this.account.user();
    return {
      id: user?.pubkey ?? '',
      username: user ? `@${user.handle}` : '',
      picture: this.dm.peers()[user?.pubkey ?? '']?.picture,
    };
  });

  constructor() {
    // My own profile, for my picture; my status.
    effect(() => {
      const me = this.account.user()?.pubkey;
      if (me) {
        void this.dm.resolve(me);
        void this.presence.load(me).catch(() => undefined);
      }
    });
  }

  public openAccount(): void {
    this.shell.settings.set(true);
  }
}
