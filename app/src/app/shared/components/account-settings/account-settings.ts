import { ChangeDetectionStrategy, Component, DestroyRef, inject, signal } from '@angular/core';
import { listen } from '@tauri-apps/api/event';
import { CmdError, LinkDone, LinkRequest } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { MediaService } from '@services/media.service';

/** Account: handle, link a device, secret key backup, log out. */
@Component({
  selector: 'app-account-settings',
  templateUrl: './account-settings.html',
  styleUrl: './account-settings.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AccountSettings {
  private readonly authService = inject(AuthService);
  protected readonly account = inject(ConnectedUserStore);
  private readonly media = inject(MediaService);
  private readonly dm = inject(DmStore);

  public readonly step = signal<'idle' | 'code' | 'confirm' | 'done' | 'failed'>('idle');
  public readonly code = signal('');
  /** The device asking for, or given, the key. */
  public readonly device = signal('');
  public readonly error = signal<CmdError['code'] | null>(null);
  public readonly nsec = signal<string | null>(null);
  public readonly confirmLogout = signal<boolean>(false);
  public readonly pictureState = signal<'idle' | 'sending' | 'done' | CmdError['code']>('idle');

  constructor() {
    const unlisten = [
      listen<LinkRequest>('link_request', ({ payload }) => {
        this.device.set(payload.device_name);
        this.step.set('confirm');
      }),
      listen<LinkDone>('link_done', ({ payload }) => {
        this.error.set(payload.error?.code ?? null);
        this.step.set(payload.error ? 'failed' : 'done');
      }),
    ];
    // Closing the modal stops the offer: the code is no longer shown.
    inject(DestroyRef).onDestroy(() => {
      unlisten.forEach((p) => p.then((stop) => stop()));
      if (this.step() === 'code' || this.step() === 'confirm') {
        this.authService.linkCancel().subscribe();
      }
    });
  }

  public offer(): void {
    this.authService.linkOffer().subscribe({
      next: (code) => {
        this.code.set(code);
        this.step.set('code');
      },
      error: (err: CmdError) => {
        this.error.set(err.code);
        this.step.set('failed');
      },
    });
  }

  public answer(accept: boolean): void {
    this.authService.linkConfirm(accept).subscribe();
  }

  /** Public picture, on my instance. */
  public setPicture(input: HTMLInputElement): void {
    const file = input.files?.[0];
    input.value = '';
    if (!file) {
      return;
    }
    this.pictureState.set('sending');
    this.media
      .upload('set_profile_picture', file)
      .then(async () => {
        const me = this.account.user()?.pubkey;
        if (me) await this.dm.resolve(me, true);
        this.pictureState.set('done');
      })
      .catch((e: CmdError) => this.pictureState.set(e.code ?? 'internal'));
  }

  public showKey(): void {
    this.authService.exportKey().subscribe((nsec) => this.nsec.set(nsec));
  }

  public logout(): void {
    // ponytail: a reload resets every store (DMs, peers) to the signed-out state.
    this.authService.logout().subscribe(() => location.assign('/'));
  }
}
