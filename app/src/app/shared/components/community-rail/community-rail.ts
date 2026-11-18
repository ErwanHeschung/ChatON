import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { NgOptimizedImage } from '@angular/common';
import { NavigationEnd, Router } from '@angular/router';
import { LucideDynamicIcon, LucideGlobe, LucidePlus } from '@lucide/angular';
import { filter } from 'rxjs';
import { ROUTES } from '@app/app.routes';
import { Avatar } from '@components/avatar/avatar';
import { CommunityDialog } from '@components/servers/community-dialog/community-dialog';
import { TooltipDirective } from '@directives/tooltip';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { Community } from '@models/Community.model';

/** Far left: private messages (the cat), then my communities, then "create or join". */
@Component({
  selector: 'app-community-rail',
  imports: [Avatar, LucideDynamicIcon, NgOptimizedImage, TooltipDirective],
  templateUrl: './community-rail.html',
  styleUrl: './community-rail.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CommunityRail implements OnInit {
  protected readonly store = inject(CommunityStore);
  private readonly router = inject(Router);
  private readonly modal = inject(ModalService);
  protected readonly shell = inject(ShellService);
  protected readonly addIcon = LucidePlus;
  protected readonly networkIcon = LucideGlobe;
  /** The DM view is open (else a community). */
  protected readonly inDms = signal(false);

  ngOnInit(): void {
    const update = () => this.inDms.set(this.router.url.includes(`/${ROUTES.DMS}`));
    update();
    this.router.events.pipe(filter((e) => e instanceof NavigationEnd)).subscribe(update);
    this.store.load().catch(() => undefined);
  }

  public openDms(): void {
    this.shell.network.set(false);
    void this.router.navigate([ROUTES.CHAT, ROUTES.DMS]);
  }

  /** The map takes the sheet (on phones, the screen) until something else is opened. */
  public toggleNetwork(): void {
    this.shell.network.update((open) => !open);
    if (this.shell.network()) this.shell.openChat();
  }

  public open(id: string): void {
    this.shell.network.set(false);
    void this.store.select(id);
    if (this.inDms()) {
      void this.router.navigate([ROUTES.CHAT, ROUTES.SERVERS]);
    }
  }

  /** A channel of it has something new from someone else. */
  protected unread(community: Community): boolean {
    return community.channels.some((c) => c.unread);
  }

  protected mentionsIn(community: Community): number {
    return community.channels.reduce((n, c) => n + c.mentions, 0);
  }

  public add(): void {
    this.modal.open(CommunityDialog);
  }
}
