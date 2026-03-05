import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';
import { AuthStatus } from '@models/Auth.model';
import { ROUTES } from 'src/app/app.routes';
import { ConnectedUserStore } from 'src/app/core/stores/connected-user.store';

export const guestGuard: CanActivateFn = () => {
  const store = inject(ConnectedUserStore);
  const router = inject(Router);

  if (store.status() === AuthStatus.AUTHENTICATED) {
    return router.parseUrl(ROUTES.CHAT);
  }
  return true;
};
