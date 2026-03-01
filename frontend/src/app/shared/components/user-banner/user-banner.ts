import { ChangeDetectionStrategy, Component } from '@angular/core';

@Component({
  selector: 'app-user-banner',
  imports: [],
  templateUrl: './user-banner.html',
  styleUrl: './user-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserBanner {}
