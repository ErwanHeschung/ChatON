import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  linkedSignal,
  signal,
} from '@angular/core';
import { LucideCheck, LucideDynamicIcon, LucideSlash, LucideX } from '@lucide/angular';
import { PERMISSION_GROUPS } from '@components/servers/permissions';
import { CmdError } from '@models/Auth.model';
import { ChannelOverride, EVERYONE, Permission } from '@models/Community.model';
import { DmStore } from '@stores/dm.store';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';

/** Slow mode choices, in seconds (spec § 9: 0 to 21600). */
const SLOWMODES = [0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21600];

/** What an override may set, by channel type, in the editor's order (spec § 9, scope « salon »). */
const TEXT_PERMS: Permission[] = [
  'view_channel',
  'send_messages',
  'attach_files',
  'add_reactions',
  'mention_everyone',
  'manage_messages',
];
const VOICE_PERMS: Permission[] = [
  'view_channel',
  'connect',
  'speak',
  'stream',
  'mute_members',
  'move_members',
];
const PERMS = PERMISSION_GROUPS.flatMap((g) => g.perms);

type Side = 'allow' | 'inherit' | 'deny';

/** A category's new id: short, random, in the spec's alphabet. */
const categoryId = () =>
  Array.from(
    crypto.getRandomValues(new Uint8Array(8)),
    (b) => '0123456789abcdefghjkmnpqrstvwxyz'[b % 32],
  ).join('');

/**
 * A channel's settings (name, topic, slow mode, limit, deletion; manage_channels) and its
 * permissions (manage_roles), or a category's (name, deletion), or a new category: whatever
 * `shell.editing` names.
 */
@Component({
  selector: 'app-channel-dialog',
  imports: [LucideDynamicIcon],
  templateUrl: './channel-dialog.html',
  styleUrls: ['../community-dialog/community-dialog.scss', './channel-dialog.scss'],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChannelDialog {
  protected readonly store = inject(CommunityStore);
  protected readonly modal = inject(ModalService);
  private readonly shell = inject(ShellService);
  private readonly dm = inject(DmStore);

  protected readonly slowmodes = SLOWMODES;
  protected readonly channel = computed(() => {
    const e = this.shell.editing();
    return e && 'channel' in e
      ? (this.store.current()?.channels.find((c) => c.id === e.channel) ?? null)
      : null;
  });
  /** The category edited; `{ id: '' }` for a new one. */
  protected readonly category = computed(() => {
    const e = this.shell.editing();
    if (!e || !('category' in e)) return null;
    if (!e.category) return { id: '', name: '' };
    return this.store.current()?.categories.find((c) => c.id === e.category) ?? null;
  });
  protected readonly state = signal<'idle' | 'saving' | CmdError['code']>('idle');
  protected readonly confirmDelete = signal(false);

  protected readonly tab = signal<'general' | 'permissions'>('general');
  protected readonly canGeneral = computed(() => {
    const ch = this.channel();
    return !!ch && this.store.canIn(ch, 'manage_channels');
  });
  protected readonly canPerms = computed(() => {
    const ch = this.channel();
    return !!ch && this.store.canIn(ch, 'manage_roles');
  });
  protected readonly shownTab = computed(() =>
    !this.canGeneral() || (this.tab() === 'permissions' && this.canPerms())
      ? 'permissions'
      : 'general',
  );

  /** The overrides being edited, saved all at once. */
  protected readonly draft = linkedSignal<ChannelOverride[]>(() => this.channel()?.overrides ?? []);
  protected readonly target = signal(EVERYONE);
  protected readonly sides = [
    { id: 'deny', icon: LucideX, label: $localize`:@@override.deny:Deny` },
    { id: 'inherit', icon: LucideSlash, label: $localize`:@@override.inherit:Inherit` },
    { id: 'allow', icon: LucideCheck, label: $localize`:@@override.allow:Allow` },
  ] as const;
  /** Who an override may name: everyone, the roles, the members; editable when below me. */
  protected readonly targets = computed(() => {
    const c = this.store.current();
    if (!c) return [];
    const has = (id: string) => this.draft().some((o) => o.target === id);
    return [
      { id: EVERYONE, label: '@everyone', editable: true, set: has(EVERYONE) },
      ...c.all_roles
        .filter((r) => r.id !== EVERYONE)
        .map((r) => ({
          id: r.id,
          label: r.name,
          editable: c.manageable_roles.includes(r.id),
          set: has(r.id),
        })),
      ...this.store
        .members()
        .filter((m) => m.manageable || has(m.pubkey))
        .map((m) => ({
          id: m.pubkey,
          label: this.dm.label(m.pubkey),
          editable: m.manageable,
          set: has(m.pubkey),
        })),
    ];
  });
  protected readonly editable = computed(
    () => this.targets().find((t) => t.id === this.target())?.editable ?? false,
  );
  protected readonly rows = computed(() => {
    const ids = this.channel()?.voice ? VOICE_PERMS : TEXT_PERMS;
    return ids.flatMap((id) => PERMS.filter((p) => p.id === id));
  });
  protected readonly isPrivate = computed(() => this.side(EVERYONE, 'view_channel') === 'deny');
  protected readonly dirty = computed(
    () => JSON.stringify(this.draft()) !== JSON.stringify(this.channel()?.overrides ?? []),
  );

  protected side(target: string, perm: Permission): Side {
    const o = this.draft().find((x) => x.target === target);
    return o?.allow.includes(perm) ? 'allow' : o?.deny.includes(perm) ? 'deny' : 'inherit';
  }

  protected set(target: string, perm: Permission, side: Side): void {
    this.draft.update((list) => {
      const others = list.filter((o) => o.target !== target);
      const o = list.find((x) => x.target === target) ?? { target, allow: [], deny: [] };
      const next = {
        target,
        allow: o.allow.filter((p) => p !== perm).concat(side === 'allow' ? [perm] : []),
        deny: o.deny.filter((p) => p !== perm).concat(side === 'deny' ? [perm] : []),
      };
      return next.allow.length || next.deny.length ? [...others, next] : others;
    });
  }

  protected togglePrivate(): void {
    this.set(EVERYONE, 'view_channel', this.isPrivate() ? 'inherit' : 'deny');
  }

  protected async savePerms() {
    const channel = this.channel();
    if (!channel) return;
    this.state.set('saving');
    const ok = await this.store.editChannel(channel.id, { overrides: this.draft() });
    this.state.set(ok ? 'idle' : (this.store.error() ?? 'internal'));
  }

  /** "30 s", "5 min", "1 h": short, in the user's language. */
  protected duration(s: number): string {
    if (!s) return $localize`:@@channel.slowmode-off:Off`;
    const [n, unit] = s < 60 ? [s, 'second'] : s < 3600 ? [s / 60, 'minute'] : [s / 3600, 'hour'];
    return new Intl.NumberFormat(undefined, { style: 'unit', unit, unitDisplay: 'short' }).format(
      n,
    );
  }

  protected async saveChannel(name: string, topic: string, slowmode: string, limit: string) {
    const channel = this.channel();
    if (!channel || !name.trim()) return;
    this.state.set('saving');
    const ok = await this.store.editChannel(channel.id, {
      name: name.trim(),
      topic: topic.trim(),
      slowmode: Number(slowmode),
      user_limit: Math.min(99, Math.max(0, Math.trunc(Number(limit) || 0))),
    });
    this.done(ok);
  }

  protected async saveCategory(name: string) {
    const c = this.store.current();
    const category = this.category();
    if (!c || !category || !name.trim()) return;
    this.state.set('saving');
    const categories = category.id
      ? c.categories.map((k) => (k.id === category.id ? { ...k, name: name.trim() } : k))
      : [...c.categories, { id: categoryId(), name: name.trim() }];
    this.done(await this.store.update({ categories }));
  }

  protected async delete() {
    const c = this.store.current();
    const [channel, category] = [this.channel(), this.category()];
    this.state.set('saving');
    if (channel) {
      this.done(await this.store.deleteChannel(channel.id));
    } else if (c && category) {
      // Its channels stay, above the categories.
      this.done(
        await this.store.update({
          categories: c.categories.filter((k) => k.id !== category.id),
          channels: c.channels.map((ch) => ({
            id: ch.id,
            category: ch.category === category.id ? null : ch.category,
          })),
        }),
      );
    }
  }

  private done(ok: boolean): void {
    if (ok) {
      this.modal.close();
    } else {
      this.state.set(this.store.error() ?? 'internal');
    }
  }
}
