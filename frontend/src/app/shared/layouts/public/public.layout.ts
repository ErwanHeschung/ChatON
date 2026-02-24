import { ChangeDetectionStrategy, Component } from '@angular/core';
import { FirefliesComponent } from '../../components/fireflies/fireflies';

@Component({
  selector: 'app-public-layout',
  imports: [FirefliesComponent],
  templateUrl: './public.layout.html',
  styleUrl: './public.layout.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PublicLayout {}
