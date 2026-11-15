import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { UserShort } from '@models/User.model';
import { NgOptimizedImage } from '@angular/common';

@Component({
  selector: 'app-user-banner',
  imports: [NgOptimizedImage],
  templateUrl: './user-banner.html',
  styleUrl: './user-banner.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserBanner {
  public user = signal<UserShort>({
    id: '1',
    username: 'TOROIMERAI',
    profilePicture: '/chatON-logo.svg',
  });
}
