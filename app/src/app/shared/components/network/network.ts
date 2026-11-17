import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  inject,
  signal,
} from '@angular/core';
import { DatePipe } from '@angular/common';
import { invoke } from '@tauri-apps/api/core';
import { LucideDynamicIcon, LucideMoon, LucideRefreshCw } from '@lucide/angular';
import { Instance } from '@models/Network.model';
import { DmStore } from '@stores/dm.store';

import { night, project } from './projection';

type World = typeof import('./world');

interface Marker {
  instance: Instance;
  x: number;
  y: number;
}

/**
 * The instances I know on a dotted world map: mine, my contacts', my communities'. Each is
 * placed by the country its admin declares (NIP-11), and core asks each whether it answers.
 */
@Component({
  selector: 'app-network',
  imports: [LucideDynamicIcon, DatePipe],
  templateUrl: './network.html',
  styleUrls: ['./network.scss', './map.scss'],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Network {
  protected readonly dm = inject(DmStore);
  protected readonly icons = { refresh: LucideRefreshCw, night: LucideMoon };
  /** The map data, in its own chunk: loaded when the map opens. */
  protected readonly world = signal<World | null>(null);
  /** Moves the night along; refreshed every few minutes and on « Check again ». */
  private readonly now = signal(new Date());
  /**
   * Sea, graticule, land and night as one still SVG image. Inline, the comets' every frame would
   * repaint them, the night's blur included: the lag. An image is drawn once. Colours come from
   * the tokens, read here since an image doesn't see the page's CSS.
   */
  protected readonly base = computed(() => {
    const world = this.world();
    if (!world) return null;
    const css = getComputedStyle(document.documentElement);
    const token = (name: string) => css.getPropertyValue(name).trim();
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${world.VIEW.join(' ')}">
<defs><clipPath id="w"><path d="${world.OUTLINE}"/></clipPath>
<filter id="d" filterUnits="userSpaceOnUse" x="-100" y="-300" width="1200" height="1100"><feGaussianBlur stdDeviation="9"/></filter></defs>
<path d="${world.OUTLINE}" fill="${token('--surface-panel')}"/>
<g clip-path="url(#w)">
<path d="${world.GRID}" fill="none" stroke="${token('--border-subtle')}" stroke-width=".6"/>
<path d="${world.LAND}" fill="${token('--surface-hover')}" stroke="${token('--border-strong')}" stroke-width=".5" stroke-linejoin="round"/>
<path d="${night(this.now())}" fill="${token('--surface-frame')}" opacity=".78" filter="url(#d)"/>
</g>
<path d="${world.OUTLINE}" fill="none" stroke="${token('--border-strong')}" stroke-width=".8"/>
</svg>`;
    return `data:image/svg+xml,${encodeURIComponent(svg)}`;
  });

  /** Mine first, then what needs attention (unreachable, blocked), then the rest. */
  protected readonly instances = signal<Instance[] | null>(null);
  protected readonly sorted = computed(() =>
    [...(this.instances() ?? [])].sort((a, b) => this.rank(a) - this.rank(b)),
  );
  /** When the last check ended. */
  protected readonly checked = signal<Date | null>(null);
  protected readonly checkedAt = computed(() => this.checked()?.getTime() ?? 0);
  protected readonly loading = signal(false);
  protected readonly failed = signal(false);
  /** Domain under the pointer, in the map or the list: both light it up. */
  protected readonly hovered = signal<string | null>(null);
  /** "FR" → "France", in the app's language (flag emojis don't exist on Windows). */
  private readonly regions = new Intl.DisplayNames([$localize.locale ?? 'en'], { type: 'region' });

  /** Placed instances; several in one country sit in a small ring around its point. */
  protected readonly markers = computed<Marker[]>(() => {
    const world = this.world();
    if (!world) return [];
    const byCountry = new Map<string, Instance[]>();
    for (const i of this.instances() ?? []) {
      const country = i.countries.find((c) => c in world.COUNTRIES);
      if (country) byCountry.set(country, [...(byCountry.get(country) ?? []), i]);
    }
    return [...byCountry].flatMap(([country, here]) => {
      const [cx, cy] = project(...world.COUNTRIES[country]);
      return here.map((instance, k) => {
        const angle = (2 * Math.PI * k) / here.length - Math.PI / 2;
        // The ring grows with the crowd, so three in one country don't touch.
        const r = here.length > 1 ? 4 + here.length * 3 : 0;
        return { instance, x: cx + r * Math.cos(angle), y: cy + r * Math.sin(angle) };
      });
    });
  });
  /** Drawn in this order: mine last, on top of any neighbour. */
  protected readonly drawn = computed(() =>
    [...this.markers()].sort((a, b) => +a.instance.mine - +b.instance.mine),
  );

  /** From my instance to each other placed one: a curve bowing up, like a flight path. */
  protected readonly links = computed(() => {
    const markers = this.markers();
    const home = markers.find((m) => m.instance.mine);
    if (!home) return [];
    return markers
      .filter((m) => m !== home)
      .map((m) => {
        const [mx, my] = [(home.x + m.x) / 2, (home.y + m.y) / 2];
        const lift = Math.hypot(m.x - home.x, m.y - home.y) * 0.25;
        // The firefly crosses in time with the round trip: a slow instance, a slow flight.
        // Square root: 20 ms and 60 ms still look different, 300 ms doesn't crawl.
        const seconds = 1.6 + Math.sqrt(m.instance.latency_ms ?? 0) / 4;
        return {
          instance: m.instance,
          d: `M${home.x} ${home.y}Q${mx} ${my - lift} ${m.x} ${m.y}`,
          dur: `${seconds.toFixed(2)}s`,
        };
      });
  });

  protected readonly unplaced = computed(
    () =>
      (this.instances() ?? []).filter((i) => !this.markers().some((m) => m.instance === i)).length,
  );
  protected readonly reachable = computed(
    () => (this.instances() ?? []).filter((i) => i.reachable && !i.blocked).length,
  );
  /** The legend only names the states on screen. */
  protected readonly states = computed(
    () => new Set((this.instances() ?? []).map((i) => this.state(i))),
  );
  /** My instance says no country: no arcs, and the view says why. */
  protected readonly homeless = computed(
    () =>
      !!this.world() && !!this.instances()?.length && !this.markers().some((m) => m.instance.mine),
  );
  /** Only my own instance: the map has nothing to connect yet. */
  protected readonly alone = computed(() => this.instances()?.length === 1);

  constructor() {
    void import('./world').then((w) => this.world.set(w));
    void this.load();
    const tick = setInterval(() => this.now.set(new Date()), 5 * 60_000);
    inject(DestroyRef).onDestroy(() => clearInterval(tick));
  }

  protected async load(): Promise<void> {
    this.loading.set(true);
    this.now.set(new Date());
    try {
      const all = await invoke<Instance[]>('instances');
      this.instances.set(all);
      this.failed.set(false);
      this.checked.set(new Date());
      // Names for « Léa, Sam: your messages will wait ».
      all
        .filter((i) => this.state(i) === 'down')
        .flatMap((i) => i.people)
        .forEach((pk) => void this.dm.resolve(pk));
    } catch {
      this.failed.set(true);
    } finally {
      this.loading.set(false);
    }
  }

  /** Who I can't reach there, by name: display name, else handle. At most three. */
  protected affected(i: Instance): string {
    const peers = this.dm.peers();
    const names = i.people.map((pk) => peers[pk]?.display_name ?? this.dm.label(pk));
    return names.length > 3 ? `${names.slice(0, 3).join(', ')}…` : names.join(', ');
  }

  /** Touch has no hover: a tap lights, the next tap elsewhere moves the light. */
  protected point(event: PointerEvent, domain: string | null): void {
    if (event.pointerType !== 'touch') this.hovered.set(domain);
  }

  private rank(i: Instance): number {
    return i.mine ? 0 : { down: 1, blocked: 2, up: 3 }[this.state(i)];
  }

  /** A firefly clicked: its row, in view and lit. */
  protected reveal(domain: string): void {
    this.hovered.set(domain);
    document.getElementById(`instance-${domain}`)?.scrollIntoView({ block: 'nearest' });
  }

  protected state(i: Instance): 'blocked' | 'up' | 'down' {
    return i.blocked ? 'blocked' : i.reachable ? 'up' : 'down';
  }

  protected where(i: Instance): string {
    return i.countries
      .filter((c) => /^[A-Z]{2}$/.test(c))
      .map((c) => this.regions.of(c) ?? c)
      .join(', ');
  }
}
