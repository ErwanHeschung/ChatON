import { ScrollingModule } from '@angular/cdk/scrolling';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
  signal,
  WritableSignal,
} from '@angular/core';
import { NgOptimizedImage } from '@angular/common';
import { TooltipDirective } from '../../directives/tooltip';
import { HorizontalScrollDirective } from '../../directives/horizontal-scroll-directives';
import { LucideAngularModule, MessagesSquare, Plus, Server } from 'lucide-angular';
import { DiscussionMode } from '../../models/DiscussionMode.model';

@Component({
  selector: 'app-horizontal-list',
  imports: [
    ScrollingModule,
    NgOptimizedImage,
    TooltipDirective,
    HorizontalScrollDirective,
    LucideAngularModule,
  ],
  templateUrl: './horizontal-list.html',
  styleUrl: './horizontal-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HorizontalList {
  readonly items = input(
    Array.from({ length: 100 }).map((_, i) => ({
      id: i,
      name: `Item ${i + 1}`,
      image: `/chatON-logo.svg`,
    })),
  );

  private readonly chatIcon = MessagesSquare;
  private readonly serverIcon = Server;

  public readonly addItem = Plus;

  public currentSelectedItem: WritableSignal<any> = signal(null);
  public discussionMode: WritableSignal<DiscussionMode> = signal(DiscussionMode.SERVER);

  public switchIcon = computed(() =>
    this.discussionMode() === DiscussionMode.SERVER ? this.chatIcon : this.serverIcon,
  );

  public switch(): void {
    this.discussionMode.update((current) =>
      current === DiscussionMode.SERVER ? DiscussionMode.DM : DiscussionMode.SERVER,
    );
  }

  public onItemClick(item: any): void {
    console.log('Clicked item:', item);
    this.currentSelectedItem.set(item);
  }
}
