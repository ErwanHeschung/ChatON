import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { ScrollingModule } from '@angular/cdk/scrolling';
import { UserShort } from '@models/User.model';
import { UserCard } from '@components/users/user-card/user-card';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { ModalService } from '@services/modal.service';
import { AccountSettings } from '@components/account-settings/account-settings';

@Component({
  selector: 'app-users-list',
  imports: [ScrollingModule, UserCard],
  templateUrl: './users-list.html',
  styleUrl: './users-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UsersList {
  private readonly account = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);

  public openAccount(): void {
    this.modalService.open(AccountSettings);
  }

  public readonly connectedUser = computed<UserShort>(() => ({
    id: this.account.user()?.npub ?? '',
    username: this.account.user() ? `@${this.account.user()?.handle}` : '',
    profilePicture: '/chatON-logo.svg',
  }));

  public readonly users = input<UserShort[]>(
    Array.from({ length: 100 }).map((_, i) => ({
      id: i.toString(),
      username: `Item ${i + 1}`,
      profilePicture: `/chatON-logo.svg`,
    })),
  );
}
