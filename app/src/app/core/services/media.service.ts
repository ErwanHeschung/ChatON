import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { CmdError } from '@models/Auth.model';
import { Attachment, MAX_FILE_SIZE } from '@models/Media.model';

/**
 * Files only go through core: it encrypts, uploads, downloads and checks hashes. The webview
 * shows `blob:` URLs made from the bytes core returns (the CSP allows no remote image).
 */
@Injectable({ providedIn: 'root' })
export class MediaService {
  /** Blob URLs by hash or picture URL; `''`: not fetched by itself, needs a click. */
  private readonly urls = new Map<string, Promise<string>>();

  /** Raw bytes as the IPC body (no JSON); `meta` goes in the `x-meta` header. */
  async upload<T>(command: string, file: File, meta: Record<string, string> = {}): Promise<T> {
    if (file.size > MAX_FILE_SIZE) {
      throw { code: 'too_large', message: 'file too large' } satisfies CmdError;
    }
    const mime = file.type || 'application/octet-stream';
    const headers = {
      'x-meta': new URLSearchParams({ ...meta, name: file.name, mime }).toString(),
    };
    return invoke<T>(command, new Uint8Array(await file.arrayBuffer()), { headers });
  }

  /** A decrypted image, fetched by itself only from my instance or my communities'. */
  image(file: Attachment): Promise<string> {
    return this.cached(file.sha256, () => this.open(file, false));
  }

  /** The user clicked: fetch it from wherever the sender put it. */
  download(file: Attachment): void {
    this.urls.set(
      file.sha256,
      this.open(file, true).catch(() => ''),
    );
  }

  /** A community or profile picture (core only fetches it from a trusted instance). */
  picture(url: string): Promise<string> {
    return this.cached(url, () =>
      invoke<ArrayBuffer>('picture', { url }).then((b) => URL.createObjectURL(new Blob([b]))),
    );
  }

  /** Asks where, then core writes the decrypted file there. False if cancelled. */
  async save(file: Attachment): Promise<boolean> {
    const path = await save({ defaultPath: file.name });
    if (path) {
      await invoke('save_attachment', { attachment: file, path });
    }
    return !!path;
  }

  private open(file: Attachment, explicit: boolean): Promise<string> {
    return invoke<ArrayBuffer>('open_attachment', { attachment: file, explicit }).then((b) =>
      URL.createObjectURL(new Blob([b], { type: file.mime })),
    );
  }

  // ponytail: blob URLs live until logout (a page reload); revoke per view if memory matters.
  private cached(key: string, load: () => Promise<string>): Promise<string> {
    let url = this.urls.get(key);
    if (!url) {
      url = load().catch(() => '');
      this.urls.set(key, url);
    }
    return url;
  }
}
