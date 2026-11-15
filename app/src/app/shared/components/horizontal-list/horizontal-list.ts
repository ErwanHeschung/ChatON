import { ScrollingModule } from '@angular/cdk/scrolling';
import { AsyncPipe } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  OnInit,
  signal,
  WritableSignal,
} from '@angular/core';
import { LucideDynamicIcon, LucideMessagesSquare, LucidePlus, LucideServer } from '@lucide/angular';
import { HorizontalScrollDirective } from '@directives/horizontal-scroll-directives';
import { TooltipDirective } from '@directives/tooltip';
import { DiscussionMode } from '@models/DiscussionMode.model';
import { Router } from '@angular/router';
import { ROUTES } from '@app/app.routes';
import { CommunityDialog } from '@components/servers/community-dialog/community-dialog';
import { Community } from '@models/Community.model';
import { ModalService } from '@services/modal.service';
import { CommunityStore } from '@stores/community.store';
import { MediaService } from '@services/media.service';

@Component({
  selector: 'app-horizontal-list',
  imports: [
    ScrollingModule,
    TooltipDirective,
    HorizontalScrollDirective,
    LucideDynamicIcon,
    AsyncPipe,
  ],
  templateUrl: './horizontal-list.html',
  styleUrl: './horizontal-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HorizontalList implements OnInit {
  protected readonly store = inject(CommunityStore);
  protected readonly media = inject(MediaService);
  private readonly router = inject(Router);
  private readonly modal = inject(ModalService);

  private readonly chatIcon = LucideMessagesSquare;
  private readonly serverIcon = LucideServer;

  public readonly addItem = LucidePlus;

  public discussionMode: WritableSignal<DiscussionMode> = signal(DiscussionMode.SERVER);

  public switchIcon = computed(() =>
    this.discussionMode() === DiscussionMode.SERVER ? this.chatIcon : this.serverIcon,
  );

  ngOnInit(): void {
    const serverPrefix = `${ROUTES.CHAT}/${ROUTES.SERVERS}`;
    this.discussionMode.set(
      this.router.url.includes(serverPrefix) ? DiscussionMode.SERVER : DiscussionMode.DM,
    );
    this.store.load().catch(() => undefined);
  }

  public switch(): void {
    this.discussionMode.update((current) =>
      current === DiscussionMode.SERVER ? DiscussionMode.DM : DiscussionMode.SERVER,
    );
    const targetPath = this.discussionMode() === DiscussionMode.DM ? ROUTES.DMS : ROUTES.SERVERS;
    void this.router.navigate([ROUTES.CHAT, targetPath]);
  }

  /** Opens a community, switching to the server view if needed. */
  public open(id: string): void {
    void this.store.select(id);
    if (this.discussionMode() !== DiscussionMode.SERVER) {
      this.switch();
    }
  }

  public add(): void {
    this.modal.open(CommunityDialog);
  }

  public trackById(_index: number, community: Community): string {
    return community.id;
  }
}
