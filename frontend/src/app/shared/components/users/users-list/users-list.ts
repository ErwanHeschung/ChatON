import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { ScrollingModule } from '@angular/cdk/scrolling';
import { UserShort } from '../../../models/User.model';
import { UserCard } from '../user-card/user-card';

@Component({
  selector: 'app-users-list',
  imports: [ScrollingModule, UserCard],
  templateUrl: './users-list.html',
  styleUrl: './users-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UsersList {
  public readonly connectedUser: UserShort = {
    id: '1',
    username: 'TOROIMERAI le bg de la mort',
    profilePicture: '/chatON-logo.svg',
  };

  public readonly users = input<UserShort[]>(
    Array.from({ length: 100 }).map((_, i) => ({
      id: i.toString(),
      username: `Item ${i + 1}`,
      profilePicture: `/chatON-logo.svg`,
    })),
  );
}
