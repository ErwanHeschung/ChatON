import { NgComponentOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { LucideDynamicIcon, LucideX } from '@lucide/angular';
import { ModalService } from '@services/modal.service';

@Component({
  selector: 'app-modal',
  imports: [NgComponentOutlet, LucideDynamicIcon],
  templateUrl: './modal.html',
  styleUrl: './modal.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown.escape)': 'modalService.close()' },
})
export class Modal {
  protected readonly modalService = inject(ModalService);
  protected readonly closeIcon = LucideX;
}
