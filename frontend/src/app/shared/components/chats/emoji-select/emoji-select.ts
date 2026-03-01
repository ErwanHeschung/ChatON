import {
  ChangeDetectionStrategy,
  Component,
  computed,
  OnInit,
  output,
  signal,
} from '@angular/core';
import compactEmojis from 'emojibase-data/en/compact.json';
import { NgOptimizedImage } from '@angular/common';
import { TwemojiUrlPipe } from '@pipes/twemoji-emoji.pipe';
import { CompactEmoji } from 'emojibase';
import { ScrollingModule } from '@angular/cdk/scrolling';
import { TooltipDirective } from '@directives/tooltip';

interface EmojiCategory {
  name: string;
  icon: CompactEmoji;
  emojis: CompactEmoji[];
}

@Component({
  selector: 'app-emoji-select',
  imports: [NgOptimizedImage, TwemojiUrlPipe, ScrollingModule, TooltipDirective],
  templateUrl: './emoji-select.html',
  styleUrl: './emoji-select.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EmojiSelect implements OnInit {
  public emojiData = signal<EmojiCategory[]>([]);
  public activeCategory = signal<string>('');
  public emojiSelect = output<CompactEmoji>();
  public columns = 7;

  currentEmojis = computed(() => {
    const category = this.emojiData().find((c) => c.name === this.activeCategory());
    return category ? category.emojis : [];
  });

  public emojiRows = computed(() => {
    const emojis = this.currentEmojis();
    const rows = [];
    for (let i = 0; i < emojis.length; i += this.columns) {
      rows.push(emojis.slice(i, i + this.columns));
    }
    return rows;
  });

  groupNames: Record<number, string> = {
    0: 'Smileys',
    1: 'People',
    3: 'Nature',
    4: 'Food',
    5: 'Places',
    6: 'Activities',
    7: 'Objects',
    8: 'Symbols',
    9: 'Flags',
  };

  ngOnInit() {
    this.organizeEmojis();
  }

  organizeEmojis() {
    const categoriesMap = new Map<string, EmojiCategory>();

    compactEmojis.forEach((item) => {
      if (item.group !== undefined) {
        const groupName = this.groupNames[item.group];

        if (groupName) {
          if (!categoriesMap.has(groupName)) {
            categoriesMap.set(groupName, {
              name: groupName,
              icon: item,
              emojis: [],
            });
          }
          categoriesMap.get(groupName)?.emojis.push(item);
        }
      }
    });

    const parsedData = Array.from(categoriesMap.values());
    this.emojiData.set(parsedData);

    if (parsedData.length > 0) {
      this.activeCategory.set(parsedData[0].name);
    }
  }
}
