import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { ModalService } from './shared/services/modal-service';
import { Modal } from './shared/components/modal/modal';
import { Titlebar } from './shared/components/titlebar/titlebar';
import { isTauri } from '@tauri-apps/api/core';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, Modal, Titlebar],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App {
  public readonly modalService: ModalService = inject(ModalService);
  /** Desktop window: native decorations are off, the app draws its own controls. */
  protected readonly desktop = isTauri() && !/Android|iPhone|iPad/.test(navigator.userAgent);
}
