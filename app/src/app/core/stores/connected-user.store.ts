import { signalStore, withState, withMethods, patchState } from '@ngrx/signals';
import { inject } from '@angular/core';
import { rxMethod } from '@ngrx/signals/rxjs-interop';
import { AuthStatus, ConnectedUser } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { catchError, of, pipe, switchMap, tap } from 'rxjs';

export const ConnectedUserStore = signalStore(
  { providedIn: 'root' },
  withState({
    user: null as ConnectedUser | null,
    status: AuthStatus.LOADING,
  }),
  withMethods((store, authService = inject(AuthService)) => ({
    refresh: rxMethod<void>(
      pipe(
        switchMap(() => {
          patchState(store, { status: AuthStatus.LOADING });
          return authService.refreshToken().pipe(
            tap((user) => {
              patchState(store, { user, status: AuthStatus.AUTHENTICATED });
            }),
            catchError(() => {
              patchState(store, { user: null, status: AuthStatus.UNAUTHENTICATED });
              return of(null);
            }),
          );
        }),
      ),
    ),
    setLoggedInUser(user: ConnectedUser) {
      patchState(store, {
        user,
        status: AuthStatus.AUTHENTICATED,
      });
    },
  })),
);
