import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { UserShort } from '@models/User.model';
import { NgOptimizedImage } from '@angular/common';

@Component({
  selector: 'app-user-card',
  imports: [NgOptimizedImage],
  templateUrl: './user-card.html',
  styleUrl: './user-card.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserCard {
  public readonly user = input.required<UserShort>();
}
