import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  inject,
  signal,
} from '@angular/core';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  LucideArrowLeft,
  LucideBell,
  LucideCheck,
  LucideCopy,
  LucideDynamicIcon,
  LucideIconInput,
  LucideInfo,
  LucideKeyRound,
  LucideMonitorSmartphone,
  LucideShieldBan,
  LucideSlidersHorizontal,
  LucideUserRound,
  LucideX,
} from '@lucide/angular';
import { LanguageToggle } from '@components/language-toggle/language-toggle';
import { ProfileCard } from '@components/users/profile-card/profile-card';
import { CmdError, LinkDone, LinkRequest } from '@models/Auth.model';
import { splitHandle } from '@models/Dm.model';
import { AuthService } from '@services/auth.service';
import { MediaService } from '@services/media.service';
import { ShellService } from '@services/shell.service';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

type Section =
  'profile' | 'account' | 'privacy' | 'devices' | 'notifications' | 'preferences' | 'about';

/**
 * Settings, full screen like the chat shell: sections in the frame, the open one in the sheet.
 * Phones show one at a time: the list, then a section with ← back.
 */
@Component({
  selector: 'app-settings',
  imports: [LucideDynamicIcon, ProfileCard, LanguageToggle],
  templateUrl: './settings.html',
  styleUrl: './settings.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    role: 'dialog',
    'aria-modal': 'true',
    'aria-labelledby': 'settings-title',
    '[class.list-open]': 'listOpen()',
    '(document:keydown.escape)': 'close()',
  },
})
export class Settings {
  private readonly authService = inject(AuthService);
  private readonly media = inject(MediaService);
  private readonly shell = inject(ShellService);
  protected readonly account = inject(ConnectedUserStore);
  protected readonly dm = inject(DmStore);

  protected readonly icons = {
    back: LucideArrowLeft,
    close: LucideX,
    copy: LucideCopy,
    copied: LucideCheck,
  };

  protected readonly sections = computed(() => {
    const all: { id: Section; label: string; icon: LucideIconInput }[] = [
      { id: 'profile', label: $localize`:@@settings.profile:Profile`, icon: LucideUserRound },
      {
        id: 'account',
        label: $localize`:@@settings.account:Account and key`,
        icon: LucideKeyRound,
      },
      { id: 'privacy', label: $localize`:@@settings.privacy:Privacy`, icon: LucideShieldBan },
      {
        id: 'devices',
        label: $localize`:@@settings.devices:Devices`,
        icon: LucideMonitorSmartphone,
      },
      { id: 'notifications', label: $localize`:@@push-title:Notifications`, icon: LucideBell },
      {
        id: 'preferences',
        label: $localize`:@@settings.preferences:Preferences`,
        icon: LucideSlidersHorizontal,
      },
      { id: 'about', label: $localize`:@@settings.about:About`, icon: LucideInfo },
    ];
    // Desktop notifies while running: nothing to set up there.
    return all.filter((s) => s.id !== 'notifications' || this.account.push() !== 'unsupported');
  });
  protected readonly section = signal<Section>('profile');
  protected readonly current = computed(() => this.sections().find((s) => s.id === this.section()));
  /** Phones: the list of sections, until one is picked. */
  protected readonly listOpen = signal(true);

  protected readonly me = computed(() => {
    const user = this.account.user();
    return (
      user && {
        ...user,
        parts: splitHandle(`@${user.handle}`),
        picture: this.dm.peers()[user.pubkey]?.picture,
        nick: this.dm.peers()[user.pubkey]?.display_name,
      }
    );
  });
  protected readonly version = signal('');
  /** Which value was just copied, for a moment: its button turns into a check. */
  protected readonly copied = signal<string | null>(null);

  public readonly step = signal<'idle' | 'code' | 'confirm' | 'done' | 'failed'>('idle');
  public readonly code = signal('');
  /** The device asking for, or given, the key. */
  public readonly device = signal('');
  public readonly error = signal<CmdError['code'] | null>(null);
  public readonly nsec = signal<string | null>(null);
  public readonly confirmLogout = signal(false);
  public readonly pictureState = signal<'idle' | 'sending' | 'done' | CmdError['code']>('idle');
  public readonly moving = signal<'idle' | 'moving' | CmdError['code']>('idle');

  constructor() {
    if (!this.dm.privacy()) void this.dm.loadPrivacy().catch(() => undefined);
    getVersion()
      .then((v) => this.version.set(v))
      .catch(() => this.version.set('dev'));
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
    // Leaving settings stops the offer: the code is no longer shown.
    inject(DestroyRef).onDestroy(() => {
      unlisten.forEach((p) => p.then((stop) => stop()).catch(() => undefined));
      if (this.step() === 'code' || this.step() === 'confirm') {
        this.authService.linkCancel().subscribe();
      }
    });
  }

  /** Moves my account to another instance (core does it all), then starts there. */
  protected move(instance: string, name: string): void {
    this.moving.set('moving');
    invoke('move_instance', { instance: instance.trim(), name: name.trim() })
      // ponytail: a reload resets every store to the new account, as after logout.
      .then(() => location.assign('/'))
      .catch((e: CmdError) => this.moving.set(e.code ?? 'internal'));
  }

  protected open(section: Section): void {
    this.section.set(section);
    this.listOpen.set(false);
  }

  protected close(): void {
    this.shell.settings.set(false);
  }

  protected async copy(value: string): Promise<void> {
    await navigator.clipboard.writeText(value);
    this.copied.set(value);
    setTimeout(() => this.copied.update((v) => (v === value ? null : v)), 1500);
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

  public readonly nickState = signal<'idle' | 'saving' | 'done' | CmdError['code']>('idle');

  /** My display name (empty removes it), in my public profile. */
  protected setNick(name: string): void {
    this.nickState.set('saving');
    invoke('set_display_name', { name })
      .then(async () => {
        const me = this.account.user()?.pubkey;
        if (me) await this.dm.resolve(me, true);
        this.nickState.set('done');
      })
      .catch((e: CmdError) => this.nickState.set(e.code ?? 'internal'));
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
