import { ChangeDetectionStrategy, Component } from '@angular/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LucideDynamicIcon, LucideMinus, LucideSquare, LucideX } from '@lucide/angular';

/** Window controls drawn by the app (native decorations are off on desktop). */
@Component({
  selector: 'app-titlebar',
  imports: [LucideDynamicIcon],
  template: `
    <div class="drag" data-tauri-drag-region></div>
    <button
      class="btn btn-ghost btn-icon btn-sm"
      i18n-title="@@window.minimize"
      title="Minimize"
      (click)="win.minimize()"
    >
      <svg [lucideIcon]="minimizeIcon" size="16"></svg>
    </button>
    <button
      class="btn btn-ghost btn-icon btn-sm"
      i18n-title="@@window.maximize"
      title="Maximize"
      (click)="win.toggleMaximize()"
    >
      <svg [lucideIcon]="maximizeIcon" size="14"></svg>
    </button>
    <button
      class="btn btn-ghost btn-icon btn-sm close"
      i18n-title="@@window.close"
      title="Close"
      (click)="win.close()"
    >
      <svg [lucideIcon]="closeIcon" size="16"></svg>
    </button>
  `,
  styles: `
    /* Floats over the page so its background shows through; layouts pad their top by its height. */
    :host {
      position: fixed;
      inset: 0 0 auto;
      z-index: calc(var(--z-modal) + 1); /* the window stays movable and closable under a dialog */
      display: flex;
      align-items: center;
      gap: var(--space-1);
      height: var(--titlebar-height);
      padding-inline-end: var(--space-1);
      user-select: none;
    }

    .drag {
      flex: 1;
      align-self: stretch;
    }

    .close:hover:not(:disabled) {
      background-color: var(--danger-subtle);
      color: var(--danger);
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Titlebar {
  protected readonly win = getCurrentWindow();
  protected readonly minimizeIcon = LucideMinus;
  protected readonly maximizeIcon = LucideSquare;
  protected readonly closeIcon = LucideX;
}
