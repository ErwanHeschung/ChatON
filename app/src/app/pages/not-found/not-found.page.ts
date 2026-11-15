import { ChangeDetectionStrategy, Component } from '@angular/core';
import { PublicLayout } from '@layouts/public/public.layout';
import { NgOptimizedImage } from '@angular/common';
import { RouterLink } from '@angular/router';

@Component({
  selector: 'app-not-found',
  imports: [PublicLayout, NgOptimizedImage, RouterLink],
  templateUrl: './not-found.page.html',
  styleUrl: './not-found.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotFound {}
