import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { AsyncPipe } from '@angular/common';
import { CommunityStore } from '@stores/community.store';
import { MediaService } from '@services/media.service';

/** Name and picture of the open community, and leaving it. */
@Component({
  selector: 'app-server-banner',
  imports: [AsyncPipe],
  templateUrl: './server-banner.html',
  styleUrl: './server-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerBanner {
  protected readonly store = inject(CommunityStore);
  protected readonly media = inject(MediaService);
  public readonly confirmLeave = signal(false);

  public setPicture(input: HTMLInputElement): void {
    const file = input.files?.[0];
    input.value = '';
    if (file) void this.store.setPicture(file);
  }

  public leave(): void {
    this.confirmLeave.set(false);
    void this.store.leave();
  }
}
