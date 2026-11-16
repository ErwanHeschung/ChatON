import {
  afterNextRender,
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  inject,
  LOCALE_ID,
  output,
  signal,
  viewChild,
} from '@angular/core';
import {
  LucideApple,
  LucideCar,
  LucideDynamicIcon,
  LucideFlag,
  LucideHand,
  LucideIconInput,
  LucideHeart,
  LucideLeaf,
  LucideLightbulb,
  LucideSmile,
  LucideTrophy,
} from '@lucide/angular';
import type { CompactEmoji, MessagesDataset } from 'emojibase';
import { TooltipDirective } from '@directives/tooltip';
import { emojiSegments } from '@pipes/emoji-text.pipe';

interface Emoji {
  unicode: string;
  /** Its Twemoji image. */
  src: string;
  label: string;
  /** Words of the label and tags, lowercase and without accents, for the search. */
  terms: string[];
}

interface Group {
  key: string;
  name: string;
  emojis: Emoji[];
}

/** Lucide, never an emoji, as the icon of a group (DESIGN.md). Group 2 (skin tones) is left out. */
const ICONS: Record<string, LucideIconInput> = {
  'smileys-emotion': LucideSmile,
  'people-body': LucideHand,
  'animals-nature': LucideLeaf,
  'food-drink': LucideApple,
  'travel-places': LucideCar,
  activities: LucideTrophy,
  objects: LucideLightbulb,
  symbols: LucideHeart,
  flags: LucideFlag,
};

/** Lowercase words without accents: « Cœur rouge » → ["coeur", "rouge"]. */
const words = (s: string) =>
  s
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .toLowerCase()
    .replace(/œ/g, 'oe')
    .replace(/æ/g, 'ae')
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);

/**
 * Twemoji, drawn like in the messages (EmojiTextPipe): what you pick is what is shown, on every OS;
 * the composer gets the emoji itself. Names and search in the app's language; data loads on first open.
 */
@Component({
  selector: 'app-emoji-select',
  imports: [LucideDynamicIcon, TooltipDirective],
  templateUrl: './emoji-select.html',
  styleUrl: './emoji-select.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EmojiSelect {
  public readonly pick = output<string>();

  protected readonly icons = ICONS;
  protected readonly groups = signal<Group[]>([]);
  protected readonly query = signal('');
  protected readonly hovered = signal<Emoji | null>(null);
  /** The group at the top of the list, as the user scrolls; its tab stays selected. */
  private readonly scrolled = signal('');
  protected readonly shown = computed(() => {
    // Every word typed starts a word of the emoji: "cat" finds 🐈, not "complicated".
    const q = words(this.query());
    if (!q.length) {
      return this.groups();
    }
    const match = (e: Emoji) => q.every((w) => e.terms.some((t) => t.startsWith(w)));
    return this.groups()
      .map((g) => ({ ...g, emojis: g.emojis.filter(match) }))
      .filter((g) => g.emojis.length > 0);
  });

  protected readonly active = computed(() => {
    const shown = this.shown();
    return shown.find((g) => g.key === this.scrolled())?.key ?? shown[0]?.key;
  });

  private readonly search = viewChild.required<ElementRef<HTMLInputElement>>('search');
  private readonly list = viewChild.required<ElementRef<HTMLElement>>('list');

  constructor() {
    void this.load(inject(LOCALE_ID).startsWith('fr'));
    afterNextRender(() => this.search().nativeElement.focus());
  }

  /** The last group whose top has scrolled past the top of the list (the last one at the end). */
  protected track(list: HTMLElement): void {
    const sections = [...list.querySelectorAll<HTMLElement>('section')];
    const end = list.scrollTop + list.clientHeight >= list.scrollHeight - 1;
    const top = end
      ? sections.at(-1)
      : sections.filter((s) => s.offsetTop <= list.scrollTop + 1).at(-1);
    this.scrolled.set(top?.dataset['group'] ?? '');
  }

  /** To a group, out of a search if there is one (after the full list is back on screen). */
  protected jump(key: string): void {
    this.query.set('');
    this.scrolled.set(key);
    requestAnimationFrame(() =>
      this.list().nativeElement.querySelector(`[data-group="${key}"]`)?.scrollIntoView(),
    );
  }

  private async load(fr: boolean): Promise<void> {
    const [data, messages] = await Promise.all(
      fr
        ? [import('emojibase-data/fr/compact.json'), import('emojibase-data/fr/messages.json')]
        : [import('emojibase-data/en/compact.json'), import('emojibase-data/en/messages.json')],
    );
    const names = (messages.default as MessagesDataset).groups;
    const groups = names
      .filter((g) => g.key in ICONS)
      .map((g) => ({ key: g.key, name: g.message, order: g.order, emojis: [] as Emoji[] }));
    // The dataset is sorted by codepoint; Unicode's order keeps 😀 first and families together.
    const emojis = [...(data.default as CompactEmoji[])].sort(
      (a, b) => (a.order ?? 0) - (b.order ?? 0),
    );
    for (const e of emojis) {
      // Only what Twemoji draws as one image (the data can be a Unicode version ahead).
      const [image, ...rest] = emojiSegments(e.unicode);
      if (!image?.src || rest.length) {
        continue;
      }
      groups
        .find((g) => g.order === e.group)
        ?.emojis.push({
          unicode: e.unicode,
          src: image.src,
          label: e.label,
          terms: words([e.label, ...(e.tags ?? [])].join(' ')),
        });
    }
    this.groups.set(groups);
  }
}
