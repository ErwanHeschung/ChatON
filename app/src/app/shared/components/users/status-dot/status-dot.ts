import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { STATUS_LABELS, Status } from '@models/Presence.model';

/**
 * A status on the corner of an avatar: online a disc, away a crescent, do not disturb a barred
 * disc, offline (and invisible) a ring. The parent is `position: relative` and sets `--dot-cut`
 * to its own background, which cuts the dot out of the avatar; `--dot-size` and `--dot-inset`
 * fit it to big avatars.
 */
@Component({
  selector: 'app-status-dot',
  template: '',
  styleUrl: './status-dot.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    role: 'img',
    '[class]': 'status()',
    '[attr.aria-label]': 'label()',
    '[attr.title]': 'label()',
  },
})
export class StatusDot {
  public readonly status = input.required<Status | 'offline'>();
  protected readonly label = computed(() => STATUS_LABELS[this.status()]);
}
