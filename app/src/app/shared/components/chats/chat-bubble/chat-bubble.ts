import { DmStore } from '@stores/dm.store';
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
  LucidePin,
  LucidePinOff,
  LucideReply,
  LucideSmilePlus,
  LucideTrash2,
} from '@lucide/angular';
import { ConnectedPosition, OverlayModule } from '@angular/cdk/overlay';
import { EmojiSelect } from '@components/chats/emoji-select/emoji-select';
import { Avatar } from '@components/avatar/avatar';
import { splitHandle } from '@models/Dm.model';
import { Attachment } from '@models/Media.model';
import { Message } from '@models/Message.model';
import { MediaService } from '@services/media.service';
import { EmojiTextPipe } from '@pipes/emoji-text.pipe';
import { ProfileDirective } from '@directives/profile';

@Component({
  selector: 'app-chat-bubble',
  imports: [
    Avatar,
    AsyncPipe,
    DatePipe,
    EmojiSelect,
    LucideDynamicIcon,
    EmojiTextPipe,
    OverlayModule,
    ProfileDirective,
  ],
  templateUrl: './chat-bubble.html',
  styleUrl: './chat-bubble.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '[class.continued]': 'continued()', '(mouseleave)': 'confirming.set(false)' },
})
export class ChatBubble {
  protected readonly media = inject(MediaService);
  private readonly dm = inject(DmStore);
  public message = input.required<Message>();
  /** Same author as the message above, shortly after: no avatar or name. */
  public readonly continued = input(false);
  public readonly remove = output<void>();
  /** Join the call this note announces. */
  public readonly join = output<void>();
  /** Answer this message. */
  public readonly reply = output<void>();
  /** Add (`on`) or take back my emoji. */
  public readonly react = output<{ emoji: string; on: boolean }>();
  /** Pin (true) or unpin. */
  public readonly pin = output<boolean>();
  /** The emoji picker of the toolbar is open: the toolbar stays. */
  protected readonly picking = signal(false);
  /** Under the button, else above it. */
  protected readonly pickerAt: ConnectedPosition[] = [
    { originX: 'end', originY: 'bottom', overlayX: 'end', overlayY: 'top', offsetY: 4 },
    { originX: 'end', originY: 'top', overlayX: 'end', overlayY: 'bottom', offsetY: -4 },
  ];
  protected readonly pinnedLabel = $localize`:@@chat.pinned:Pinned`;
  protected readonly pinTitle = $localize`:@@chat.pin:Pin`;
  protected readonly unpinTitle = $localize`:@@chat.unpin:Unpin`;
  /** First click asks, second deletes. */
  public readonly confirming = signal(false);
  public readonly saveFailed = signal(false);
  protected readonly icons = {
    file: LucideFile,
    delete: LucideTrash2,
    save: LucideDownload,
    call: LucidePhone,
    callEnded: LucidePhoneOff,
    reply: LucideReply,
    react: LucideSmilePlus,
    pin: LucidePin,
    unpin: LucidePinOff,
  };

  /** From the picker: one emoji, then it closes. Already mine: nothing to add. */
  protected pickReaction(emoji: string): void {
    this.picking.set(false);
    if (!this.message().reactions?.some((r) => r.emoji === emoji && r.mine)) {
      this.react.emit({ emoji, on: true });
    }
  }
  protected readonly author = computed(() => splitHandle(this.message().senderName));
  /** Their display name, with the handle quieter after it. */
  protected readonly nick = computed(
    () => this.dm.peers()[this.message().senderId]?.display_name ?? null,
  );
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
