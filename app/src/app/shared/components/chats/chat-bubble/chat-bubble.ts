import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';
import { AsyncPipe, NgOptimizedImage } from '@angular/common';
import { Attachment } from '@models/Media.model';
import { Message } from '@models/Message.model';
import { MediaService } from '@services/media.service';

@Component({
  selector: 'app-chat-bubble',
  imports: [NgOptimizedImage, AsyncPipe],
  templateUrl: './chat-bubble.html',
  styleUrl: './chat-bubble.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ChatBubble {
  protected readonly media = inject(MediaService);
  public message = input.required<Message>();
  public readonly remove = output<void>();
  /** First click asks, second deletes. */
  public readonly confirming = signal(false);
  public readonly saveFailed = signal(false);

  public isImage(file: Attachment): boolean {
    return file.mime.startsWith('image/');
  }

  public size(bytes: number): string {
    return bytes < 1024 * 1024
      ? `${Math.max(1, Math.round(bytes / 1024))} KB`
      : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  public save(file: Attachment): void {
    this.saveFailed.set(false);
    this.media.save(file).catch(() => this.saveFailed.set(true));
  }
}
