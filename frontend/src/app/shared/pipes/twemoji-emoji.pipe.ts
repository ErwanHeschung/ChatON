import { Pipe, PipeTransform } from '@angular/core';
@Pipe({
  name: 'twemojiUrl',
})
export class TwemojiUrlPipe implements PipeTransform {
  private readonly baseUrl = 'https://cdn.jsdelivr.net/gh/jdecked/twemoji@latest/assets/svg/';

  transform(hexcode: string | undefined): string {
    if (!hexcode) return '';

    const fileName = hexcode.toLowerCase();

    return `${this.baseUrl}${fileName}.svg`;
  }
}
