import { Component, inject } from '@angular/core';
import { ModalService } from '../../modal-service';
import { NgComponentOutlet } from '@angular/common';

@Component({
  selector: 'app-modal',
  imports: [NgComponentOutlet],
  templateUrl: './modal.html',
  styleUrl: './modal.scss',
})
export class Modal {
  protected readonly modalService = inject(ModalService);
}
