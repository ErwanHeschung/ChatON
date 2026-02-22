import { NgOptimizedImage } from '@angular/common';
import { ChangeDetectionStrategy, Component } from '@angular/core';
import { LanguageToggle } from '../../shared/components/language-toggle/language-toggle';
import { Public } from '../../shared/layouts/public/public.layout';

@Component({
 selector: 'app-landing-page',
 imports: [NgOptimizedImage, LanguageToggle, Public],
 templateUrl: './landing.page.html',
 styleUrl: './landing.page.scss',
 changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LandingPage {}
