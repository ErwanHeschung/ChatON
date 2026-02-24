import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PublicLayout } from '../../shared/layouts/public/public.layout';
import { NgOptimizedImage } from '@angular/common';
import { RouterLink } from '@angular/router';

@Component({
  selector: 'app-not-found',
  imports: [PublicLayout, NgOptimizedImage, RouterLink],
  templateUrl: './not-found.html',
  styleUrl: './not-found.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotFound {}
