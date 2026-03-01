import { ChangeDetectionStrategy, Component } from '@angular/core';
import { NgOptimizedImage } from "@angular/common";

@Component({
  selector: 'app-server-banner',
  imports: [NgOptimizedImage],
  templateUrl: './server-banner.html',
  styleUrl: './server-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ServerBanner {}
