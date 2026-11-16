import { Overlay, OverlayRef } from '@angular/cdk/overlay';
import { ComponentPortal } from '@angular/cdk/portal';
import { DestroyRef, Directive, ElementRef, inject, Injector, input, signal } from '@angular/core';
import { ProfilePopover } from '@components/users/profile-popover/profile-popover';

/** On a button: a click shows that person's card beside it. Outside click or Escape closes it. */
@Directive({
  selector: '[appProfile]',
  host: {
    'aria-haspopup': 'dialog',
    '[attr.aria-expanded]': 'open()',
    '(click)': 'toggle()',
  },
})
export class ProfileDirective {
  public readonly pubkey = input.required<string>({ alias: 'appProfile' });

  private readonly overlay = inject(Overlay);
  private readonly elementRef = inject(ElementRef<HTMLElement>);
  private readonly injector = inject(Injector);
  private overlayRef: OverlayRef | null = null;
  protected readonly open = signal(false);

  constructor() {
    inject(DestroyRef).onDestroy(() => this.overlayRef?.dispose());
  }

  protected toggle(): void {
    if (this.overlayRef) {
      this.close();
      return;
    }
    // Beside the trigger: to the right in the conversation, to the left from the members panel.
    const positionStrategy = this.overlay
      .position()
      .flexibleConnectedTo(this.elementRef)
      .withPositions([
        { originX: 'end', originY: 'top', overlayX: 'start', overlayY: 'top', offsetX: 12 },
        { originX: 'start', originY: 'top', overlayX: 'end', overlayY: 'top', offsetX: -12 },
      ])
      .withPush(true)
      .withViewportMargin(16);
    this.overlayRef = this.overlay.create({
      positionStrategy,
      scrollStrategy: this.overlay.scrollStrategies.close(),
      hasBackdrop: true,
      backdropClass: 'cdk-overlay-transparent-backdrop',
    });
    this.overlayRef.backdropClick().subscribe(() => this.close());
    this.overlayRef.keydownEvents().subscribe((e) => e.key === 'Escape' && this.close());
    const card = this.overlayRef.attach(new ComponentPortal(ProfilePopover, null, this.injector));
    card.setInput('pubkey', this.pubkey());
    card.instance.done.subscribe(() => this.close());
    this.open.set(true);
  }

  /** Back to the trigger, for the keyboard. */
  private close(): void {
    this.overlayRef?.dispose();
    this.overlayRef = null;
    this.open.set(false);
    this.elementRef.nativeElement.focus();
  }
}
