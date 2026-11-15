import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';
import { UserShort } from '@models/User.model';
import { AsyncPipe, NgOptimizedImage } from '@angular/common';
import { MediaService } from '@services/media.service';

@Component({
  selector: 'app-user-card',
  imports: [NgOptimizedImage, AsyncPipe],
  templateUrl: './user-card.html',
  styleUrl: './user-card.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UserCard {
  protected readonly media = inject(MediaService);
  public readonly user = input.required<UserShort>();
}
