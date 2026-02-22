import { Component } from '@angular/core';
import { Public } from '../../shared/layouts/public/public.layout';
import { NgOptimizedImage } from '@angular/common';
import { RouterLink } from '@angular/router';

@Component({
 selector: 'app-not-found',
 imports: [Public, NgOptimizedImage, RouterLink],
 templateUrl: './not-found.html',
 styleUrl: './not-found.scss',
})
export class NotFound {}
