import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { DmStore } from '@stores/dm.store';
import { LucideCamera, LucideDynamicIcon } from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { StatusDot } from '@components/users/status-dot/status-dot';
import { PresenceStore } from '@stores/presence.store';
import { splitHandle } from '@models/Dm.model';

/**
 * Someone's card: banner, avatar, handle. The banner is the app's background until profiles
 * carry a custom one. Editable: the avatar opens the file picker.
 */
@Component({
  selector: 'app-profile-card',
  imports: [Avatar, LucideDynamicIcon, StatusDot],
  templateUrl: './profile-card.html',
  styleUrl: './profile-card.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfileCard {
  /** `@pseudo@instance`. */
  public readonly handle = input.required<string>();
  public readonly pubkey = input.required<string>();
  public readonly picture = input<string | null | undefined>(null);
  /** My own card: cat ears. */
  public readonly mine = input(false);
  /** The avatar is a button to change the picture (settings). */
  public readonly editable = input(false);
  public readonly busy = input(false);
  public readonly editPicture = output<void>();

  protected readonly cameraIcon = LucideCamera;
  protected readonly parts = computed(() => splitHandle(this.handle()));
  protected readonly nick = computed(() => this.dm.peers()[this.pubkey()]?.display_name ?? null);
  private readonly dm = inject(DmStore);
  protected readonly presence = inject(PresenceStore);
}
