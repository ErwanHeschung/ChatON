import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { CmdError } from '@models/Auth.model';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';

/** Slow mode choices, in seconds (spec § 9: 0 to 21600). */
const SLOWMODES = [0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21600];

/** A category's new id: short, random, in the spec's alphabet. */
const categoryId = () =>
  Array.from(
    crypto.getRandomValues(new Uint8Array(8)),
    (b) => '0123456789abcdefghjkmnpqrstvwxyz'[b % 32],
  ).join('');

/**
 * A channel's settings (name, topic, slow mode, limit, deletion), or a category's (name,
 * deletion), or a new category: whatever `shell.editing` names. manage_channels.
 */
@Component({
  selector: 'app-channel-dialog',
  templateUrl: './channel-dialog.html',
  styleUrls: ['../community-dialog/community-dialog.scss', './channel-dialog.scss'],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChannelDialog {
  protected readonly store = inject(CommunityStore);
  protected readonly modal = inject(ModalService);
  private readonly shell = inject(ShellService);

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
