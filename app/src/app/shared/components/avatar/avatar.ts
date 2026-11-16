import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { AsyncPipe } from '@angular/common';
import { MediaService } from '@services/media.service';

/**
 * A person or a community: its picture (fetched through core) or, without one, its initial on a
 * color derived from `seed`, so the same key always gets the same color.
 */
@Component({
  selector: 'app-avatar',
  imports: [AsyncPipe],
  templateUrl: './avatar.html',
  styleUrl: './avatar.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '[class]': 'size() + " " + shape()',
    '[class.ears]': 'ears()',
    '[style.--avatar-hue]': 'hue()',
  },
})
export class Avatar {
  protected readonly media = inject(MediaService);
  /** Remote picture URL; null shows the initial. */
  public readonly picture = input<string | null | undefined>(null);
  public readonly name = input.required<string>();
  /** A stable id (pubkey, community id); defaults to the name. */
  public readonly seed = input('');
  public readonly size = input<'sm' | 'md' | 'lg' | 'xl' | '2xl'>('md');
  public readonly shape = input<'circle' | 'square'>('circle');
  /** Cat ears: marks me. */
  public readonly ears = input(false);

  /** First letter or digit: "@lea@chaton.fr" gives "L". */
  protected readonly initial = computed(
    () =>
      this.name()
        .match(/[\p{L}\p{N}]/u)?.[0]
        ?.toUpperCase() ?? '?',
  );

  protected readonly hue = computed(() => {
    let hash = 0;
    for (const char of this.seed() || this.name()) {
      hash = (hash * 31 + char.charCodeAt(0)) % 360;
    }
    return hash;
  });
}
