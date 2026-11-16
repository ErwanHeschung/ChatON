import { ChangeDetectionStrategy, Component, computed, effect, inject } from '@angular/core';
import { LucideDynamicIcon, LucideSettings } from '@lucide/angular';
import { UserCard } from '@components/users/user-card/user-card';
import { UserShort } from '@models/User.model';
import { ShellService } from '@services/shell.service';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** Bottom of the sidebar: who I am, and my account settings. */
@Component({
  selector: 'app-user-panel',
  imports: [UserCard, LucideDynamicIcon],
  templateUrl: './user-panel.html',
  styleUrl: './user-panel.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserPanel {
  private readonly account = inject(ConnectedUserStore);
  private readonly dm = inject(DmStore);
  private readonly shell = inject(ShellService);
  protected readonly settingsIcon = LucideSettings;

  protected readonly me = computed<UserShort>(() => {
    const user = this.account.user();
    return {
      id: user?.pubkey ?? '',
      username: user ? `@${user.handle}` : '',
      picture: this.dm.peers()[user?.pubkey ?? '']?.picture,
    };
  });

  constructor() {
    // My own profile, for my picture.
    effect(() => {
      const me = this.account.user()?.pubkey;
      if (me) void this.dm.resolve(me);
    });
  }

  public openAccount(): void {
    this.shell.settings.set(true);
  }
}
