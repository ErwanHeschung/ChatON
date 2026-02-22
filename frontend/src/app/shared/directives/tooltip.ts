import { Overlay, OverlayRef } from '@angular/cdk/overlay';
import { ComponentPortal } from '@angular/cdk/portal';
import { Directive, ElementRef, inject, input } from '@angular/core';
import { Tooltip } from '../components/tooltip/tooltip';

@Directive({
  selector: '[appTooltip]',
  host: {
    '(mouseenter)': 'show()',
    '(mouseleave)': 'hide()',
    '(mousedown)': 'hide()',
  },
})
export class TooltipDirective {
  text = input.required<string>({ alias: 'appTooltip' });

  private readonly overlay = inject(Overlay);
  private readonly elementRef = inject(ElementRef);
  private overlayRef: OverlayRef | null = null;

  show() {
    if (this.overlayRef) return;

    const positionStrategy = this.overlay
      .position()
      .flexibleConnectedTo(this.elementRef)
      .withPositions([
        { originX: 'center', originY: 'top', overlayX: 'center', overlayY: 'bottom', offsetY: -8 },
        { originX: 'center', originY: 'bottom', overlayX: 'center', overlayY: 'top', offsetY: 8 },
      ]);

    this.overlayRef = this.overlay.create({
      positionStrategy,
      scrollStrategy: this.overlay.scrollStrategies.reposition(),
      panelClass: 'tooltip-container',
    });

    const portal = new ComponentPortal(Tooltip);
    const componentRef = this.overlayRef.attach(portal);

    componentRef.instance.content.set(this.text());
  }

  hide() {
    if (this.overlayRef) {
      this.overlayRef.detach();
      this.overlayRef = null;
    }
  }
}
