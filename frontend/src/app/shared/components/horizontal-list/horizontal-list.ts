import { ScrollingModule } from '@angular/cdk/scrolling';
import { NgOptimizedImage } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  OnInit,
  signal,
  WritableSignal,
} from '@angular/core';
import { LucideAngularModule, MessagesSquare, Plus, Server } from 'lucide-angular';
import { HorizontalScrollDirective } from '@directives/horizontal-scroll-directives';
import { TooltipDirective } from '@directives/tooltip';
import { DiscussionMode } from '@models/DiscussionMode.model';
import { Router } from '@angular/router';
import { ROUTES } from 'src/app/app.routes';

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
export class HorizontalList implements OnInit {
  readonly items = input(
    Array.from({ length: 100 }).map((_, i) => ({
      id: i,
      name: `Item ${i + 1}`,
      image: `/chatON-logo.svg`,
    })),
  );

  private readonly router = inject(Router);

  private readonly chatIcon = MessagesSquare;
  private readonly serverIcon = Server;

  public readonly addItem = Plus;

  public currentSelectedItem: WritableSignal<any> = signal(null);
  public discussionMode: WritableSignal<DiscussionMode> = signal(DiscussionMode.SERVER);

  public switchIcon = computed(() =>
    this.discussionMode() === DiscussionMode.SERVER ? this.chatIcon : this.serverIcon,
  );

  ngOnInit(): void {
    const currentUrl = this.router.url;
    const serverPrefix = `${ROUTES.CHAT}/${ROUTES.SERVERS}`;

    if (currentUrl.includes(serverPrefix)) {
      this.discussionMode.set(DiscussionMode.SERVER);
    } else {
      this.discussionMode.set(DiscussionMode.DM);
    }
  }

  public switch(): void {
    this.discussionMode.update((current) =>
      current === DiscussionMode.SERVER ? DiscussionMode.DM : DiscussionMode.SERVER,
    );

    const targetPath = this.discussionMode() === DiscussionMode.DM ? ROUTES.DMS : ROUTES.SERVERS;

    this.router.navigate([ROUTES.CHAT + '/' + targetPath]);
  }

  public onItemClick(item: any): void {
    console.log('Clicked item:', item);
    this.currentSelectedItem.set(item);
  }
}
