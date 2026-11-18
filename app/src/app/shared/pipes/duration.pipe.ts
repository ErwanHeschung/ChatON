import { inject, LOCALE_ID, Pipe, PipeTransform } from '@angular/core';

/** Seconds as "30 s", "5 min", "1 h", "7 d", in the user's language; 0: "Off". */
@Pipe({ name: 'duration' })
export class DurationPipe implements PipeTransform {
  private readonly locale = inject(LOCALE_ID);

  transform(s: number): string {
    if (!s) return $localize`:@@channel.slowmode-off:Off`;
    const [n, unit] =
      s < 60
        ? [s, 'second']
        : s < 3600
          ? [s / 60, 'minute']
          : s < 86400
            ? [s / 3600, 'hour']
            : [s / 86400, 'day'];
    return new Intl.NumberFormat(this.locale, { style: 'unit', unit, unitDisplay: 'short' }).format(
      n,
    );
  }
}
