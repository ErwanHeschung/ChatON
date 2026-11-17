import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { Avatar } from '@components/avatar/avatar';
import { splitHandle } from '@models/Dm.model';
import { UserShort } from '@models/User.model';

/** Avatar and name, the instance dimmed: `@lea` `@chaton.fr`. The row around it is the caller's. */
@Component({
  selector: 'app-user-card',
  imports: [Avatar],
  templateUrl: './user-card.html',
  styleUrl: './user-card.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserCard {
  public readonly user = input.required<UserShort>();
  public readonly size = input<'sm' | 'md'>('sm');
  /** Cat ears on the avatar: it's me. */
  public readonly ears = input(false);
  /** In a voice room, talking right now: a green ring. */
  public readonly speaking = input(false);
  protected readonly parts = computed(() => splitHandle(this.user().username));
}
