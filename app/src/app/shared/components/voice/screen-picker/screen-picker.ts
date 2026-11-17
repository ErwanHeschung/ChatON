import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { LucideAppWindow, LucideDynamicIcon, LucideMonitor } from '@lucide/angular';
import { ScreenSource } from '@models/Voice.model';
import { ModalService } from '@services/modal.service';
import { VoiceStore } from '@stores/voice.store';

/** Share screen → which screen or window (where the OS has no picker of its own). */
@Component({
  selector: 'app-screen-picker',
  imports: [LucideDynamicIcon],
  templateUrl: './screen-picker.html',
  styleUrl: './screen-picker.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ScreenPicker {
  protected readonly voice = inject(VoiceStore);
  private readonly modal = inject(ModalService);
  protected readonly icons = { screen: LucideMonitor, window: LucideAppWindow };

  protected readonly screens = computed(() => (this.voice.sources() ?? []).filter((s) => s.screen));
  protected readonly windows = computed(() =>
    (this.voice.sources() ?? []).filter((s) => !s.screen && s.title),
  );

  protected pick(source: ScreenSource): void {
    this.modal.close();
    void this.voice.pick(source);
  }
}
