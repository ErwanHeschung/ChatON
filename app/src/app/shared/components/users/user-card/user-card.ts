import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { DmStore } from '@stores/dm.store';
import { Avatar } from '@components/avatar/avatar';
import { StatusDot } from '@components/users/status-dot/status-dot';
import { PresenceStore } from '@stores/presence.store';
import { splitHandle } from '@models/Dm.model';
import { UserShort } from '@models/User.model';

/** Avatar and name, the instance dimmed: `@lea` `@chaton.fr`. The row around it is the caller's. */
@Component({
  selector: 'app-user-card',
  imports: [Avatar, StatusDot],
  templateUrl: './user-card.html',
  styleUrl: './user-card.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserCard {
  public readonly user = input.required<UserShort>();
  public readonly size = input<'sm' | 'md'>('sm');
  /** Cat ears on the avatar: it's me. */
  public readonly ears = input(false);
  /** Their highest role's color, on the name (the instance stays dimmed). */
  public readonly color = input<string | null | undefined>(null);
  /** In a voice room, talking right now: a green ring. */
  public readonly speaking = input(false);
  /** A status dot on the avatar (a person, not a group). */
  public readonly presence = input(false);
  protected readonly status = computed(() =>
    this.presence() ? this.presenceStore.of(this.user().id) : null,
  );
  private readonly presenceStore = inject(PresenceStore);
  protected readonly parts = computed(() => splitHandle(this.user().username));
  protected readonly nick = computed(() => this.dm.peers()[this.user().id]?.display_name ?? null);
  private readonly dm = inject(DmStore);
}
