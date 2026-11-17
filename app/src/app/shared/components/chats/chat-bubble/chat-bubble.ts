import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { AsyncPipe, DatePipe } from '@angular/common';
import {
  LucideDownload,
  LucideDynamicIcon,
  LucideFile,
  LucidePhone,
  LucidePhoneOff,
  LucideTrash2,
} from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { splitHandle } from '@models/Dm.model';
import { Attachment } from '@models/Media.model';
import { Message } from '@models/Message.model';
import { MediaService } from '@services/media.service';
import { EmojiTextPipe } from '@pipes/emoji-text.pipe';
import { ProfileDirective } from '@directives/profile';

@Component({
  selector: 'app-chat-bubble',
  imports: [Avatar, AsyncPipe, DatePipe, LucideDynamicIcon, EmojiTextPipe, ProfileDirective],
  templateUrl: './chat-bubble.html',
  styleUrl: './chat-bubble.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '[class.continued]': 'continued()', '(mouseleave)': 'confirming.set(false)' },
})
export class ChatBubble {
  protected readonly media = inject(MediaService);
  public message = input.required<Message>();
  /** Same author as the message above, shortly after: no avatar or name. */
  public readonly continued = input(false);
  public readonly remove = output<void>();
  /** Join the call this note announces. */
  public readonly join = output<void>();
  /** First click asks, second deletes. */
  public readonly confirming = signal(false);
  public readonly saveFailed = signal(false);
  protected readonly icons = {
    file: LucideFile,
    delete: LucideTrash2,
    save: LucideDownload,
    call: LucidePhone,
    callEnded: LucidePhoneOff,
  };
  protected readonly author = computed(() => splitHandle(this.message().senderName));
  /** The time today, the day and time before. */
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
