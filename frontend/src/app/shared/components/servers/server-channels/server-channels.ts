import { ScrollingModule } from '@angular/cdk/scrolling';
import { UpperCasePipe } from '@angular/common';
import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';
import { Category, Channel } from '@models/Channel.model';
import { ChevronDown, Hash, LucideAngularModule } from 'lucide-angular';
import { ServerBanner } from '@components/servers/server-banner/server-banner';

type RenderItem =
  | { type: 'category'; data: Category; expanded: boolean }
  | { type: 'channel'; data: Channel };

export const MOCK_CATEGORIES: Category[] = [
  { id: 'cat-1', serverId: 'srv-100', name: 'Information & Rules' },
  { id: 'cat-2', serverId: 'srv-100', name: 'Text Channels — General Discussion' },
  { id: 'cat-3', serverId: 'srv-100', name: 'Voice Lounges' },
  { id: 'cat-4', serverId: 'srv-100', name: 'Gaming Zone (Pro Players Only)' },
  { id: 'cat-5', serverId: 'srv-100', name: 'Development & Bug Reports' },
  {
    id: 'cat-6',
    serverId: 'srv-100',
    name: 'EXTREMELY_LONG_CATEGORY_NAME_THAT_NEVER_ENDS_SO_WE_CAN_TEST_THE_ELLIPSIS_UI',
  },
  { id: 'cat-7', serverId: 'srv-100', name: 'Archive 2024' },
  { id: 'cat-8', serverId: 'srv-100', name: 'Hidden Staff Area' },
];

export const MOCK_CHANNELS: Channel[] = [
  // Cat 1: Info
  { id: 'ch-1', serverId: 'srv-100', categoryId: 'cat-1', name: 'welcome-and-rules' },
  { id: 'ch-2', serverId: 'srv-100', categoryId: 'cat-1', name: 'announcements' },
  { id: 'ch-3', serverId: 'srv-100', categoryId: 'cat-1', name: 'roles-assignment' },

  // Cat 2: General
  { id: 'ch-4', serverId: 'srv-100', categoryId: 'cat-2', name: 'general-chat' },
  { id: 'ch-5', serverId: 'srv-100', categoryId: 'cat-2', name: 'memes-and-offtopic' },
  {
    id: 'ch-6',
    serverId: 'srv-100',
    categoryId: 'cat-2',
    name: 'this-channel-name-is-way-too-long-to-fit-in-any-normal-sidebar-width',
  },
  { id: 'ch-7', serverId: 'srv-100', categoryId: 'cat-2', name: 'tech-support' },

  // Cat 3: Voice
  { id: 'ch-8', serverId: 'srv-100', categoryId: 'cat-3', name: 'General Voice' },
  { id: 'ch-9', serverId: 'srv-100', categoryId: 'cat-3', name: 'Music Bot 1' },
  { id: 'ch-10', serverId: 'srv-100', categoryId: 'cat-3', name: 'AFK - Sleeping' },

  // Cat 4: Gaming
  { id: 'ch-11', serverId: 'srv-100', categoryId: 'cat-4', name: 'league-of-legends-rankeds' },
  {
    id: 'ch-12',
    serverId: 'srv-100',
    categoryId: 'cat-4',
    name: 'counter-strike-global-offensive',
  },
  { id: 'ch-13', serverId: 'srv-100', categoryId: 'cat-4', name: 'minecraft-creative-builds' },
  { id: 'ch-14', serverId: 'srv-100', categoryId: 'cat-4', name: 'valorant-clips' },

  // Cat 5: Dev
  { id: 'ch-15', serverId: 'srv-100', categoryId: 'cat-5', name: 'angular-signals-help' },
  { id: 'ch-16', serverId: 'srv-100', categoryId: 'cat-5', name: 'css-grid-vs-flexbox' },
  { id: 'ch-17', serverId: 'srv-100', categoryId: 'cat-5', name: 'typescript-errors-log' },

  // Cat 6: Stress Test
  {
    id: 'ch-18',
    serverId: 'srv-100',
    categoryId: 'cat-6',
    name: 'overflow-test-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  },
  {
    id: 'ch-19',
    serverId: 'srv-100',
    categoryId: 'cat-6',
    name: 'overflow-test-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
  },

  // Cat 7: Archive
  { id: 'ch-20', serverId: 'srv-100', categoryId: 'cat-7', name: 'old-general' },
  { id: 'ch-21', serverId: 'srv-100', categoryId: 'cat-7', name: 'deleted-projects' },
];

@Component({
  selector: 'app-server-channels',
  imports: [ScrollingModule, UpperCasePipe, LucideAngularModule, ServerBanner],
  templateUrl: './server-channels.html',
  styleUrl: './server-channels.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerChannels {
  public categories = input<Category[]>(MOCK_CATEGORIES);
  public channels = input<Channel[]>(MOCK_CHANNELS);

  public collapsedCategories = signal<Set<string>>(new Set());

  public chevronIcon = ChevronDown;
  public hashtagIcon = Hash;

  public virtualItems = computed<RenderItem[]>(() => {
    const flatList: RenderItem[] = [];
    const collapsed = this.collapsedCategories();

    for (const cat of this.categories()) {
      const isExpanded = !collapsed.has(cat.id);

      flatList.push({ type: 'category', data: cat, expanded: isExpanded });

      if (isExpanded) {
        const catChannels = this.channels().filter((ch) => ch.categoryId === cat.id);
        catChannels.forEach((ch) => flatList.push({ type: 'channel', data: ch }));
      }
    }
    return flatList;
  });

  public toggleCategory(id: string): void {
    this.collapsedCategories.update((set) => {
      const newSet = new Set(set);
      if (newSet.has(id)) newSet.delete(id);
      else newSet.add(id);
      return newSet;
    });
  }

  trackById(_index: number, item: RenderItem): string {
    return item.type === 'category' ? `cat-${item.data.id}` : `ch-${item.data.id}`;
  }
}
