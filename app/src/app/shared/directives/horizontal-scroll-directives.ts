import { Directive, ElementRef, inject } from '@angular/core';

@Directive({
  selector: '[appHorizontalScroll]',
  host: {
    '(wheel)': 'onWheel($event)',
  },
})
export class HorizontalScrollDirective {
  private readonly el = inject(ElementRef);

  onWheel(event: WheelEvent) {
    const el = this.el.nativeElement;
    event.preventDefault();

    el.scrollBy({
      left: event.deltaY,
    });
  }
}
