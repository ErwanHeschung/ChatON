import { Routes } from '@angular/router';

export const ROUTES = {
  PUBLIC: '/',
  CHAT: 'chat',
  SERVERS: 'servers',
  DMS: 'dms',
};

export const routes: Routes = [
  {
    path: '',
    loadComponent: () => import('@pages/landing/landing.page').then((m) => m.LandingPage),
  },
  {
    path: ROUTES.CHAT,
    loadComponent: () => import('@pages/root-chat/root-chat.page').then((m) => m.RootChat),
    children: [
      {
        path: ROUTES.SERVERS,
        children: [
          {
            path: '',
            loadComponent: () =>
              import('@components/servers/server-channels/server-channels').then(
                (m) => m.ServerChannels,
              ),
            outlet: 'left-sidebar',
          },
        ],
      },
      {
        path: ROUTES.DMS,
        children: [
          {
            path: '',
            loadComponent: () =>
              import('@components/users/user-banner/user-banner').then((m) => m.UserBanner),
            outlet: 'left-sidebar',
          },
        ],
      },
      { path: '', redirectTo: ROUTES.SERVERS, pathMatch: 'full' },
    ],
  },
  {
    path: '**',
    loadComponent: () => import('@pages/not-found/not-found.page').then((m) => m.NotFound),
  },
];
