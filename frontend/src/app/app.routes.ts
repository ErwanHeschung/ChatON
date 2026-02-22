import { Routes } from '@angular/router';

export const routes: Routes = [
 {
  path: '',
  loadComponent: () => import('./pages/landing.page/landing.page').then((m) => m.LandingPage),
 },
 {
  path: '**',
  loadComponent: () => import('./pages/not-found/not-found').then((m) => m.NotFound),
 },
];
