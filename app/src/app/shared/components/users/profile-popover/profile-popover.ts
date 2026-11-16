import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
} from '@angular/core';
import { Router } from '@angular/router';
import { LucideDynamicIcon, LucideMessageCircle, LucidePencil } from '@lucide/angular';
import { ROUTES } from '@app/app.routes';
import { ProfileCard } from '@components/users/profile-card/profile-card';
import { roomKey } from '@models/Dm.model';
import { ShellService } from '@services/shell.service';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** Someone's card, floating next to where they were clicked (ProfileDirective). */
@Component({
  selector: 'app-profile-popover',
  imports: [ProfileCard, LucideDynamicIcon],
  template: `
    <app-profile-card [handle]="handle()" [pubkey]="pubkey()" [picture]="picture()" [mine]="mine()">
      <div class="actions">
        @if (mine()) {
          <button type="button" class="btn" (click)="editProfile()">
            <svg [lucideIcon]="editIcon" size="16"></svg>
            <ng-container i18n="@@profile.edit">Edit profile</ng-container>
          </button>
        } @else {
          <button type="button" class="btn btn-primary" (click)="message()">
            <svg [lucideIcon]="messageIcon" size="16"></svg>
            <ng-container i18n="@@profile.message">Send a message</ng-container>
          </button>
        }
      </div>
    </app-profile-card>
  `,
  styles: `
    :host {
      display: block;
      width: min(20rem, 100vw - 2 * var(--space-4));
      border-radius: var(--radius-lg);
      box-shadow: var(--shadow-popover);
    }

    .actions {
      display: flex;
      width: 100%;
      margin-top: var(--space-3);

      .btn {
        flex: 1;
      }
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfilePopover {
  private readonly dm = inject(DmStore);
  private readonly account = inject(ConnectedUserStore);
  private readonly shell = inject(ShellService);
  private readonly router = inject(Router);

  public readonly pubkey = input.required<string>();
  /** Asks the directive to close the card. */
  public readonly done = output<void>();

  protected readonly editIcon = LucidePencil;
  protected readonly messageIcon = LucideMessageCircle;
  protected readonly handle = computed(() => this.dm.label(this.pubkey()));
  protected readonly picture = computed(() => this.dm.peers()[this.pubkey()]?.picture);
  protected readonly mine = computed(() => this.pubkey() === this.account.user()?.pubkey);

  constructor() {
    // A community member may not be resolved yet: their handle and picture come in.
    effect(() => void this.dm.resolve(this.pubkey()));
  }

  protected editProfile(): void {
    this.shell.settings.set(true);
    this.done.emit();
  }

  /** Opens (or starts) the 1:1, from wherever we are. */
  protected async message(): Promise<void> {
    this.done.emit();
    await this.router.navigate([ROUTES.CHAT, ROUTES.DMS]);
    // Core sorts rooms by key bytes: the same order as lowercase hex strings.
    await this.dm.select(roomKey([this.account.user()?.pubkey ?? '', this.pubkey()].sort()));
    this.shell.openChat();
  }
}
