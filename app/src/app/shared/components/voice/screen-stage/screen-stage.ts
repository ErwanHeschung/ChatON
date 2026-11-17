import {
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  inject,
  OnDestroy,
  OnInit,
  viewChild,
} from '@angular/core';
import { LucideDynamicIcon, LucideMaximize, LucideX } from '@lucide/angular';
import { splitHandle } from '@models/Dm.model';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';
import { VoiceStore } from '@stores/voice.store';

/** The screen I watch, in place of the conversation. Images come from core as JPEG. */
@Component({
  selector: 'app-screen-stage',
  imports: [LucideDynamicIcon],
  templateUrl: './screen-stage.html',
  styleUrl: './screen-stage.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ScreenStage implements OnInit, OnDestroy {
  protected readonly voice = inject(VoiceStore);
  private readonly dm = inject(DmStore);
  private readonly user = inject(ConnectedUserStore);
  protected readonly icons = { fullscreen: LucideMaximize, close: LucideX };
  private readonly canvas = viewChild.required<ElementRef<HTMLCanvasElement>>('screen');

  /** My own share, as the others see it. */
  protected readonly mine = computed(
    () => this.voice.state()?.watching === this.user.user()?.pubkey,
  );

  /** Whose screen: `@pseudo` and `@instance`. */
  protected readonly who = computed(() =>
    splitHandle(this.dm.label(this.voice.state()?.watching ?? '')),
  );

  /** An image still decoding: the next ones are dropped, the stream keeps the newest. */
  private drawing = false;

  ngOnInit(): void {
    this.voice.onImage((jpeg) => {
      if (this.drawing) {
        return;
      }
      this.drawing = true;
      createImageBitmap(new Blob([jpeg], { type: 'image/jpeg' }))
        .then((image) => {
          const canvas = this.canvas().nativeElement;
          if (canvas.width !== image.width || canvas.height !== image.height) {
            canvas.width = image.width;
            canvas.height = image.height;
          }
          canvas.getContext('2d')?.drawImage(image, 0, 0);
          image.close();
        })
        .catch(() => undefined)
        .finally(() => (this.drawing = false));
    });
  }

  ngOnDestroy(): void {
    this.voice.onImage(null);
  }

  protected fullscreen(): void {
    void this.canvas()
      .nativeElement.requestFullscreen()
      .catch(() => undefined);
  }
}
