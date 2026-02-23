import { ScrollingModule } from '@angular/cdk/scrolling';
import { Component, input, signal, WritableSignal } from '@angular/core';
import { NgOptimizedImage } from '@angular/common';
import { TooltipDirective } from '../../directives/tooltip';
import { HorizontalScrollDirective } from '../../directives/horizontal-scroll-directives';
import { LucideAngularModule, MessagesSquare, Plus } from 'lucide-angular';

@Component({
  selector: 'app-horizontal-list',
  imports: [ScrollingModule, NgOptimizedImage, TooltipDirective, HorizontalScrollDirective, LucideAngularModule],
  templateUrl: './horizontal-list.html',
  styleUrl: './horizontal-list.scss',
})
export class HorizontalList {
  readonly items = input(
    Array.from({ length: 100 }).map((_, i) => ({
      id: i,
      name: `Item ${i + 1}`,
      image: `/chatON-logo.svg`,
    })),
  );

  public readonly chatIcon = MessagesSquare;
  public readonly addItem = Plus;
  public currentSelectedItem: WritableSignal<any> = signal(null);

  public onItemClick(item: any): void {
    console.log('Clicked item:', item);
    this.currentSelectedItem.set(item);
  }
}
