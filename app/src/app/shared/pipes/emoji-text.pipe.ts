import { Pipe, PipeTransform } from '@angular/core';
import { parse } from '@twemoji/parser';

/** A run of text, or one emoji with its Twemoji image (`text` stays the emoji, for `alt`). */
export interface Segment {
  text: string;
  src?: string;
}

/** The one Twemoji file named without the FE0Fs the parser keeps (👁️‍🗨️): checked over every emoji. */
const RENAMED: Record<string, string> = { '1f441-fe0f-200d-1f5e8-fe0f': '1f441-200d-1f5e8' };

/** Twemoji's SVGs, fetched at build time into public/twemoji (scripts/twemoji.mjs). */
export const twemojiUrl = (codepoints: string) =>
  `twemoji/${RENAMED[codepoints] ?? codepoints}.svg`;

/** Splits text into plain runs and Twemoji emoji, so every OS draws the same emoji. */
export function emojiSegments(text: string): Segment[] {
  const segments: Segment[] = [];
  let at = 0;
  for (const e of parse(text, { buildUrl: twemojiUrl })) {
    const [start, end] = e.indices;
    if (start > at) {
      segments.push({ text: text.slice(at, start) });
    }
    segments.push({ text: e.text, src: e.url });
    at = end;
  }
  if (at < text.length) {
    segments.push({ text: text.slice(at) });
  }
  return segments;
}

@Pipe({ name: 'emojiText' })
export class EmojiTextPipe implements PipeTransform {
  transform(text: string): Segment[] {
    return emojiSegments(text);
  }
}
