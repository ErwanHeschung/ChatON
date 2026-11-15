import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';
import { LucideDynamicIcon, LucidePawPrint } from '@lucide/angular';

/** A cat walked by: paw prints that light up one after the other. Decorative. */
@Component({
  selector: 'app-paw-trail',
  imports: [LucideDynamicIcon],
  template: `@for (step of steps(); track step) {
    <svg [lucideIcon]="paw" [size]="size()" [style.--step]="step"></svg>
  }`,
  styleUrl: './paw-trail.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { 'aria-hidden': 'true' },
})
export class PawTrail {
  protected readonly paw = LucidePawPrint;
  public readonly length = input(6);
  public readonly size = input(18);
  protected readonly steps = computed(() => Array.from({ length: this.length() }, (_, i) => i));
}
